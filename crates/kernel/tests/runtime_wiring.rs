use std::sync::Arc;
use std::time::Duration;

use cdm::Entity;
use fabric::{
    BlastRadiusDto, EnvelopeCreateRequest, EnvelopeService, GovernorProvider, StoreEnvelopeService,
};
use governor::{
    ActionEnvelope, BlastRadius, Constitution, Decision, EnvelopeState, InvocationContext, Level,
    Reversal, SpendSnapshot,
};
use hydra_kernel::policy_provider::PersistedGovernorProvider;
use hydra_kernel::runtime_services::{
    LlmRuntimeConfig, RuntimeAvailability, RuntimeBuildError, RuntimeServices,
};
use serde_json::json;
use store::{Store, TestDb};
use tokio::sync::watch;
use uuid::Uuid;

#[tokio::test]
async fn runtime_wiring_refreshes_persisted_governor_policy(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let provider = PersistedGovernorProvider::new(store.autonomy.clone(), constitution());
        let envelope = stage_change_envelope(tenant, Uuid::new_v4());
        let spend = SpendSnapshot {
            month_to_date_cents: 0,
        };

        let initial = provider.governor(tenant).await?;
        assert!(matches!(
            initial.evaluate(&envelope, &spend),
            Decision::SuggestOnly
        ));

        store
            .autonomy
            .upsert_cell(
                tenant,
                "pipeline",
                "move_stage",
                Some("deal"),
                Level::L2,
                &json!({"batch_max": 25}),
            )
            .await?;
        let queued = provider.governor(tenant).await?;
        assert!(matches!(
            queued.evaluate(&envelope, &spend),
            Decision::Queue
        ));

        store
            .autonomy
            .upsert_cell(
                tenant,
                "pipeline",
                "move_stage",
                Some("deal"),
                Level::L4,
                &json!({"batch_max": 25}),
            )
            .await?;
        let executable = provider.governor(tenant).await?;
        assert!(matches!(
            executable.evaluate(&envelope, &spend),
            Decision::Execute(_)
        ));
        assert_eq!(provider.cached_tenants().await, 1);

        store
            .autonomy
            .set_frozen(tenant, true, Some("runtime policy test"), "operator:test")
            .await?;
        let frozen = provider.governor(tenant).await?;
        assert!(matches!(
            frozen.evaluate(&envelope, &spend),
            Decision::SuggestOnly
        ));

        store
            .autonomy
            .set_frozen(tenant, false, None, "operator:test")
            .await?;
        let thawed = provider.governor(tenant).await?;
        assert!(matches!(
            thawed.evaluate(&envelope, &spend),
            Decision::Execute(_)
        ));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn runtime_wiring_supervises_the_real_executor_worker(
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
                    body: json!({"title": "Runtime renewal", "stage_id": "discovery"}),
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
                &json!({"batch_max": 25}),
            )
            .await?;

        let provider: Arc<dyn GovernorProvider> = Arc::new(PersistedGovernorProvider::new(
            store.autonomy.clone(),
            constitution(),
        ));
        let (runtime, worker) = RuntimeServices::build(store.clone())?;
        let executor_health = runtime.executor_health.clone();
        assert!(!executor_health.running());
        assert_eq!(
            runtime.components.bridge_host,
            RuntimeAvailability::Available
        );
        assert!(matches!(
            runtime.components.bridge_lifecycle,
            RuntimeAvailability::Unavailable(_)
        ));
        assert!(matches!(
            runtime.components.tokenkiller_router,
            RuntimeAvailability::Disabled(_)
        ));
        assert!(matches!(
            runtime.components.data_steward,
            RuntimeAvailability::Experimental(_)
        ));
        assert!(matches!(
            runtime.components.bridge_engineer,
            RuntimeAvailability::Disabled(_)
        ));
        assert_eq!(
            runtime.components.comms_draft,
            RuntimeAvailability::Available
        );
        assert!(matches!(
            runtime.components.comms_transport,
            RuntimeAvailability::Unavailable(_)
        ));
        assert!(runtime
            .execution_registry
            .runtime_capabilities()
            .contains(fabric::capabilities::RUNTIME_PIPELINE_MOVE_STAGE_DEAL));

        let envelopes = StoreEnvelopeService::with_governor_provider(store.clone(), provider)
            .with_execution_dispatcher(runtime.dispatcher.clone());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let worker_handle = tokio::spawn(worker.run(runtime.executor.clone(), shutdown_rx));
        wait_for_health(&executor_health, true).await;
        assert!(executor_health.available());

        let proposed = envelopes
            .propose(
                tenant,
                EnvelopeCreateRequest {
                    domain: "pipeline".to_owned(),
                    action: "move_stage".to_owned(),
                    kind: Some("deal".to_owned()),
                    targets: vec![deal.id],
                    payload: json!({"stage": "won"}),
                    rationale: "prove real runtime execution".to_owned(),
                    reversal: Reversal::Compensating,
                    blast: BlastRadiusDto {
                        entities: 1,
                        external_sends: 0,
                        money_cents: 0,
                        pii_egress: false,
                    },
                },
            )
            .await?;
        assert_eq!(proposed.state, EnvelopeState::Approved);

        let executed = wait_for_state(&store, tenant, proposed.id, EnvelopeState::Executed).await?;
        assert_eq!(executed.state, EnvelopeState::Executed);
        assert_eq!(
            store.entities.get(tenant, deal.id).await?.body["stage_id"],
            "won"
        );

        shutdown_tx.send(true)?;
        worker_handle.await?;
        assert!(!executor_health.available());
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn configured_provider_constructs_experimental_bridge_synthesis_runtime(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let (runtime, _worker) = RuntimeServices::build_with_config(
            store,
            LlmRuntimeConfig {
                openai_compat_base_url: Some("http://127.0.0.1:9".to_owned()),
                openai_compat_model: Some("test-mapping-model".to_owned()),
                output_budget_bytes: 4096,
                ..Default::default()
            },
        )?;
        assert!(matches!(
            runtime.components.bridge_engineer,
            RuntimeAvailability::Experimental(_)
        ));
        assert!(runtime.bridge_synthesis.is_some());
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn runtime_wiring_recovers_durable_approved_envelope_without_dispatch_token(
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
                    body: json!({"title": "Restart recovery", "stage_id": "discovery"}),
                    origin: "native".to_owned(),
                    origin_ref: None,
                    version: 1,
                },
            )
            .await?;
        let proposed = stage_change_envelope(tenant, deal.id);
        store.envelopes.save(tenant, &proposed).await?;
        store
            .envelopes
            .transition(
                tenant,
                proposed.id,
                EnvelopeState::Approved,
                "governor",
                &TestClock,
            )
            .await?;

        let (runtime, worker) = RuntimeServices::build(store.clone())?;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let worker_handle = tokio::spawn(worker.run(runtime.executor.clone(), shutdown_rx));

        let executed = wait_for_state(&store, tenant, proposed.id, EnvelopeState::Executed).await?;
        assert_eq!(executed.state, EnvelopeState::Executed);
        assert_eq!(
            store.entities.get(tenant, deal.id).await?.body["stage_id"],
            "won"
        );

        shutdown_tx.send(true)?;
        worker_handle.await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn runtime_wiring_concurrent_recovery_executes_one_receipt(
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
                    body: json!({"title": "Concurrent recovery", "stage_id": "discovery"}),
                    origin: "native".to_owned(),
                    origin_ref: None,
                    version: 1,
                },
            )
            .await?;
        let proposed = stage_change_envelope(tenant, deal.id);
        store.envelopes.save(tenant, &proposed).await?;
        store
            .envelopes
            .transition(
                tenant,
                proposed.id,
                EnvelopeState::Approved,
                "governor",
                &TestClock,
            )
            .await?;

        let (runtime_a, worker_a) = RuntimeServices::build(store.clone())?;
        let (runtime_b, worker_b) = RuntimeServices::build(store.clone())?;
        let (shutdown_a, shutdown_rx_a) = watch::channel(false);
        let (shutdown_b, shutdown_rx_b) = watch::channel(false);
        let worker_handle_a = tokio::spawn(worker_a.run(runtime_a.executor.clone(), shutdown_rx_a));
        let worker_handle_b = tokio::spawn(worker_b.run(runtime_b.executor.clone(), shutdown_rx_b));

        let executed = wait_for_state(&store, tenant, proposed.id, EnvelopeState::Executed).await?;
        assert_eq!(executed.state, EnvelopeState::Executed);
        let receipt_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM execution_receipt WHERE tenant_id = $1 AND envelope_id = $2",
        )
        .bind(tenant)
        .bind(proposed.id)
        .fetch_one(&db.pool)
        .await?;
        assert_eq!(receipt_count, 1);
        assert_eq!(store.entities.get(tenant, deal.id).await?.version, 2);

        shutdown_a.send(true)?;
        shutdown_b.send(true)?;
        worker_handle_a.await?;
        worker_handle_b.await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn runtime_wiring_constructs_configured_tokenkiller_router_without_provider_io(
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://hydra:hydra@127.0.0.1:55432/hydra")?;
    let (runtime, _worker) = RuntimeServices::build_with_config(
        Store::new(pool),
        LlmRuntimeConfig {
            openai_compat_base_url: Some("http://127.0.0.1:9/v1".to_owned()),
            openai_compat_model: Some("fixture-model".to_owned()),
            output_budget_bytes: 4_096,
            ..LlmRuntimeConfig::default()
        },
    )?;
    assert_eq!(
        runtime.components.bridge_host,
        RuntimeAvailability::Available
    );
    assert!(matches!(
        runtime.components.bridge_lifecycle,
        RuntimeAvailability::Unavailable(_)
    ));
    assert_eq!(
        runtime.components.tokenkiller_router,
        RuntimeAvailability::Available
    );
    assert_eq!(
        runtime.components.skill_discovery,
        RuntimeAvailability::Disabled("signed skill discovery is not configured".to_owned())
    );
    assert!(runtime.skill_registry.is_none());
    Ok(())
}

#[tokio::test]
async fn runtime_wiring_rejects_partial_skill_configuration(
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://hydra:hydra@127.0.0.1:55432/hydra")?;
    let result = RuntimeServices::build_with_config(
        Store::new(pool),
        LlmRuntimeConfig {
            skills_path: Some("/tmp/hydra-skills".to_owned()),
            ..LlmRuntimeConfig::default()
        },
    );
    assert!(matches!(result, Err(RuntimeBuildError::SkillConfig(_))));
    Ok(())
}

async fn wait_for_state(
    store: &Store,
    tenant: Uuid,
    envelope_id: Uuid,
    expected: EnvelopeState,
) -> Result<ActionEnvelope, Box<dyn std::error::Error>> {
    for _ in 0..100 {
        let envelope = store.envelopes.get(tenant, envelope_id).await?;
        if envelope.state == expected {
            return Ok(envelope);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("envelope {envelope_id} did not reach {expected:?}").into())
}

async fn wait_for_health(health: &hydra_kernel::supervisor::TaskHealth, expected: bool) {
    for _ in 0..100 {
        if health.available() == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(health.available(), expected);
}

fn stage_change_envelope(tenant: Uuid, target: Uuid) -> ActionEnvelope {
    ActionEnvelope {
        id: Uuid::new_v4(),
        tenant,
        domain: "pipeline".to_owned(),
        action: "move_stage".to_owned(),
        kind: Some("deal".to_owned()),
        targets: vec![target],
        payload: json!({"stage": "won"}),
        rationale: "policy refresh test".to_owned(),
        reversal: Reversal::Compensating,
        blast: BlastRadius {
            entities: 1,
            ..BlastRadius::default()
        },
        invocation: InvocationContext::default(),
        state: EnvelopeState::Proposed,
        history: Vec::new(),
    }
}

struct TestClock;

impl governor::Clock for TestClock {
    fn now(&self) -> time::OffsetDateTime {
        time::OffsetDateTime::now_utc()
    }
}

fn constitution() -> Constitution {
    Constitution {
        monthly_spend_cap_cents: 50_000,
        pii_egress_allowlist: vec!["private".to_owned()],
        blast_entities_ceiling: 250,
        blast_sends_ceiling: 50,
        blast_money_ceiling_cents: 250_000,
    }
}
