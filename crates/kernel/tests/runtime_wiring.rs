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
use hydra_kernel::runtime_services::{LlmRuntimeConfig, RuntimeAvailability, RuntimeServices};
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
            RuntimeAvailability::Unavailable(_)
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

fn constitution() -> Constitution {
    Constitution {
        monthly_spend_cap_cents: 50_000,
        pii_egress_allowlist: vec!["private".to_owned()],
        blast_entities_ceiling: 250,
        blast_sends_ceiling: 50,
        blast_money_ceiling_cents: 250_000,
    }
}
