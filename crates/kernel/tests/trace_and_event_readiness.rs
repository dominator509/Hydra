use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::http::{HeaderMap, HeaderValue};
use cdm::Entity;
use fabric::{
    BlastRadiusDto, CapabilityRegistry, CorrelationContext, EnvelopeCreateRequest, EnvelopeService,
    EventStatusService, GovernedExternalProposal, GovernorProvider, PrincipalContext,
    PrincipalType, Scope, StoreEnvelopeService,
};
use governor::{Constitution, EnvelopeState, Level, Reversal};
use hydra_kernel::event_status::{required_event_infrastructure_ready, EventRuntimeStatusService};
use hydra_kernel::event_stream::{
    EventPublishAck, EventPublishRequest, EventPublisher, EventStreamConfig, EventStreamError,
    JetStreamEventPublisher,
};
use hydra_kernel::policy_provider::PersistedGovernorProvider;
use hydra_kernel::relay::{publish_once, run_with_health, RelayHealth};
use hydra_kernel::runtime_services::RuntimeServices;
use serde_json::json;
use store::{Store, TestDb, TraceContext};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
struct CapturingPublisher {
    requests: Mutex<Vec<EventPublishRequest>>,
}

#[async_trait]
impl EventPublisher for CapturingPublisher {
    async fn publish(
        &self,
        request: EventPublishRequest,
    ) -> Result<EventPublishAck, EventStreamError> {
        self.requests.lock().await.push(request);
        Ok(EventPublishAck {
            stream: "TRACE_TEST".to_owned(),
            sequence: 1,
            duplicate: false,
        })
    }
}

#[test]
fn trace_and_event_readiness_parser_creates_child_and_rejects_invalid_parent() {
    let mut valid_headers = HeaderMap::new();
    valid_headers.insert(
        "traceparent",
        HeaderValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
    );
    valid_headers.insert("tracestate", HeaderValue::from_static("nexus=opaque"));
    valid_headers.insert("baggage", HeaderValue::from_static("customer=private"));
    let child = fabric::trace_context::server_trace_context(&valid_headers);
    assert_eq!(child.trace_id(), "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_ne!(
        child.traceparent,
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
    );
    assert_eq!(child.tracestate.as_deref(), Some("nexus=opaque"));

    let mut invalid_headers = HeaderMap::new();
    invalid_headers.insert("traceparent", HeaderValue::from_static("invalid"));
    let fresh = fabric::trace_context::server_trace_context(&invalid_headers);
    assert!(fresh.validate().is_ok());
    assert_ne!(fresh.trace_id(), child.trace_id());
}

#[tokio::test]
async fn trace_and_event_readiness_full_governed_path_preserves_correlation(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let deal = store
            .entities
            .upsert(
                tenant,
                Entity {
                    id: Uuid::new_v4(),
                    kind: "deal".to_owned(),
                    tenant,
                    body: json!({"title": "Trace fixture", "stage_id": "discovery"}),
                    origin: "native".to_owned(),
                    origin_ref: None,
                    version: 1,
                },
            )
            .await?;
        store
            .autonomy
            .upsert_cell(
                tenant,
                "pipeline",
                "move_stage",
                Some("deal"),
                Level::L4,
                &json!({"batch_max": 1}),
            )
            .await?;

        let provider: Arc<dyn GovernorProvider> = Arc::new(PersistedGovernorProvider::new(
            store.autonomy.clone(),
            test_constitution(),
        ));
        let (runtime, worker) = RuntimeServices::build(store.clone())?;
        let service = StoreEnvelopeService::with_governor_provider(store.clone(), provider)
            .with_execution_dispatcher(runtime.dispatcher.clone());
        let mut runtime_capabilities = runtime.execution_registry.runtime_capabilities();
        runtime_capabilities
            .insert(fabric::capabilities::RUNTIME_TENANT_SCOPED_ENVELOPE_GET.to_owned());
        let registry =
            CapabilityRegistry::nexus_v1_with_runtime_capabilities(runtime_capabilities)?;
        let capability = registry
            .get("hydra.crm.propose_action")
            .ok_or("proposal capability missing")?;
        let request_trace = TraceContext::new(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            Some("nexus=test".to_owned()),
        )?;
        let principal = PrincipalContext {
            principal_id: "nexus-service:trace".to_owned(),
            principal_type: PrincipalType::NexusService,
            hydra_tenant_id: tenant,
            external_provider: Some("nexus".to_owned()),
            external_tenant_id: Some("nexus-tenant-trace".to_owned()),
            external_business_id: Some("business-trace".to_owned()),
            scopes: BTreeSet::from([Scope::CrmPropose]),
            delegated_by: None,
            authentication_strength: Some("service".to_owned()),
            token_id: Some("token-trace".to_owned()),
            correlation: CorrelationContext {
                request_id: "request-trace".to_owned(),
                correlation_id: "correlation-trace".to_owned(),
                causation_id: Some("causation-trace".to_owned()),
            },
            binding_id: Some(Uuid::new_v4()),
            trace: request_trace.clone(),
        };

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let worker_handle = tokio::spawn(worker.run(runtime.executor.clone(), shutdown_rx));
        let proposed = service
            .propose_external(
                &principal,
                capability,
                GovernedExternalProposal {
                    request: EnvelopeCreateRequest {
                        domain: "pipeline".to_owned(),
                        action: "move_stage".to_owned(),
                        kind: Some("deal".to_owned()),
                        targets: vec![deal.id],
                        payload: json!({"stage": "won"}),
                        rationale: "trace propagation acceptance".to_owned(),
                        reversal: Reversal::Compensating,
                        blast: BlastRadiusDto {
                            entities: 1,
                            external_sends: 0,
                            money_cents: 0,
                            pii_egress: false,
                        },
                    },
                    idempotency_key: "trace-stage-change-1".to_owned(),
                    request_hash: "a".repeat(64),
                    objective_id: Some("objective-trace".to_owned()),
                    task_id: Some("task-trace".to_owned()),
                },
            )
            .await?;
        wait_for_state(&store, tenant, proposed.id, EnvelopeState::Executed).await?;

        let publisher = CapturingPublisher::default();
        let iteration =
            publish_once(&store.outbox, &publisher, 100, Duration::from_secs(30)).await?;
        assert!(iteration.published >= 4);
        let requests = publisher.requests.lock().await;
        let correlated = requests
            .iter()
            .filter(|request| {
                serde_json::from_slice::<cdm::HydraEventEnvelope>(&request.payload)
                    .is_ok_and(|event| event.correlation_id.as_deref() == Some("correlation-trace"))
            })
            .collect::<Vec<_>>();
        assert!(correlated.len() >= 3);
        let mut downstream_child_seen = false;
        for request in correlated {
            let event: cdm::HydraEventEnvelope = serde_json::from_slice(&request.payload)?;
            assert_eq!(event.causation_id.as_deref(), Some("causation-trace"));
            assert!(!serde_json::to_string(&event.payload)?.contains("traceparent"));
            let trace = request
                .trace_context
                .as_ref()
                .ok_or("correlated event missing trace context")?;
            assert_eq!(trace.trace_id(), request_trace.trace_id());
            downstream_child_seen |= trace.traceparent != request_trace.traceparent;
        }
        assert!(downstream_child_seen);

        shutdown_tx.send(true)?;
        worker_handle.await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn trace_and_event_readiness_required_stream_outage_fails_closed(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".to_owned());
    let client = async_nats::connect(&nats_url).await?;
    let context = async_nats::jetstream::new(client.clone());
    let stream_name = format!(
        "HYDRA_TRACE_READY_{}",
        Uuid::new_v4().simple().to_string().to_uppercase()
    );
    let mut config = EventStreamConfig::nexus_v1();
    config.name.clone_from(&stream_name);
    config.subjects = vec![format!("hydra.test.trace.{}.>", Uuid::new_v4().simple())];
    config.max_age = Duration::from_secs(300);
    config.max_messages = 1_000;
    config.max_bytes = 10 * 1024 * 1024;
    config.duplicate_window = Duration::from_secs(120);
    config.storage = async_nats::jetstream::stream::StorageType::Memory;
    let publisher = JetStreamEventPublisher::bootstrap(client, config).await?;
    let relay_health = RelayHealth::default();
    let status = EventRuntimeStatusService::new(publisher.clone(), relay_health.clone());
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let relay_handle = tokio::spawn(run_with_health(
        shutdown_rx,
        Store::new(db.pool.clone()).outbox,
        Arc::new(publisher),
        relay_health.clone(),
    ));

    for _ in 0..50 {
        if relay_health.running() && relay_health.operational() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(status.status().await?.available);
    assert!(required_event_infrastructure_ready(true, &status).await);

    context.delete_stream(&stream_name).await?;
    let unavailable = status.status().await?;
    assert!(!unavailable.available);
    assert_eq!(
        unavailable.reason.as_deref(),
        Some("canonical_event_stream_unavailable")
    );
    assert!(!required_event_infrastructure_ready(true, &status).await);
    assert!(required_event_infrastructure_ready(false, &status).await);

    shutdown_tx.send(true)?;
    relay_handle.await?;
    db.cleanup().await?;
    Ok(())
}

async fn wait_for_state(
    store: &Store,
    tenant: Uuid,
    envelope_id: Uuid,
    expected: EnvelopeState,
) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..100 {
        if store.envelopes.get(tenant, envelope_id).await?.state == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("envelope {envelope_id} did not reach {expected:?}").into())
}

fn test_constitution() -> Constitution {
    Constitution {
        monthly_spend_cap_cents: 50_000,
        pii_egress_allowlist: vec!["private".to_owned()],
        blast_entities_ceiling: 250,
        blast_sends_ceiling: 50,
        blast_money_ceiling_cents: 250_000,
    }
}
