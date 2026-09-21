use std::path::PathBuf;
use std::sync::Arc;

use fabric::BridgeConformanceService;
use governor::{
    ActionEnvelope, BlastRadius, Cell, Clock, Constitution, EnvelopeState, Governor, Level,
    PolicyMatrix, Reversal, SpendSnapshot,
};
use hydra_kernel::runtime_services::{LlmRuntimeConfig, RuntimeAvailability, RuntimeServices};
use serde_json::json;
use store::{BridgeAdapterState, Store, TestDb};
use time::OffsetDateTime;
use uuid::Uuid;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }
}

#[tokio::test]
async fn configured_runtime_executes_deploy_pause_resume_and_idempotent_redeploy(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let (disabled_runtime, _disabled_worker) =
            RuntimeServices::build_with_config(store.clone(), LlmRuntimeConfig::default())?;
        assert!(disabled_runtime.bridge_conformance.is_none());
        let adapters_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters");
        let (runtime, _worker) = RuntimeServices::build_with_config_and_secrets(
            store.clone(),
            LlmRuntimeConfig {
                adapters_path: Some(adapters_path.to_string_lossy().into_owned()),
                egress_proxy_url: Some("http://127.0.0.1:9".to_owned()),
                ..LlmRuntimeConfig::default()
            },
            Arc::new(bridge_host::StaticSecretSource::default()),
            RuntimeAvailability::Available,
        )?;

        assert_eq!(
            runtime.components.bridge_lifecycle,
            RuntimeAvailability::Available
        );
        let capabilities = runtime.execution_registry.runtime_capabilities();
        assert!(capabilities.contains(fabric::capabilities::RUNTIME_BRIDGE_DEPLOY_ADAPTER));
        assert!(capabilities.contains(fabric::capabilities::RUNTIME_BRIDGE_PAUSE_ADAPTER));
        assert!(capabilities.contains(fabric::capabilities::RUNTIME_BRIDGE_RESUME_ADAPTER));
        assert!(capabilities.contains(fabric::capabilities::RUNTIME_BRIDGE_SYNC_ADAPTER));
        let capability_registry =
            fabric::CapabilityRegistry::nexus_v1_with_runtime_capabilities(capabilities)?;
        assert!(
            capability_registry
                .get("hydra.bridges.deploy")
                .expect("deploy capability exists")
                .available
        );

        let governor = bridge_governor();
        let payload = json!({
            "adapter_id": "memcrm",
            "wiring_ref": "memcrm.wasm",
            "grant": {
                "origins": [],
                "secret_names": [],
                "dsn_name": null,
                "fuel": 100_000
            }
        });
        let deployed = execute_action(
            &store,
            runtime.executor.as_ref(),
            &governor,
            tenant,
            "deploy_adapter",
            Reversal::Compensating,
            payload.clone(),
        )
        .await?;
        assert_eq!(deployed.state, EnvelopeState::Executed);

        let record = store
            .bridge_adapters
            .get(tenant, "memcrm")
            .await?
            .expect("deploy creates durable adapter record");
        assert_eq!(record.state, BridgeAdapterState::Active);
        assert!(record.descriptor.is_some());
        assert_eq!(
            store.bridge_adapters.history(tenant, "memcrm").await?.len(),
            3
        );

        let conformance = runtime
            .bridge_conformance
            .as_ref()
            .expect("configured runtime exposes bridge conformance");
        let conformance_result = conformance
            .conform(
                tenant,
                json!({"adapterId": "memcrm", "kind": "party", "limit": 25}),
            )
            .await?;
        assert_eq!(conformance_result["checked_kind"], "party");
        assert_eq!(conformance_result["listed_record_count"], 0);
        assert!(conformance_result.get("descriptor").is_some());
        let after_conformance = store
            .bridge_adapters
            .get(tenant, "memcrm")
            .await?
            .expect("adapter remains registered after read-only conformance");
        assert_eq!(after_conformance.revision, record.revision);
        assert!(store
            .bridge_sync
            .current(tenant, "memcrm", "party")
            .await?
            .is_none());

        let cross_tenant_conformance = conformance
            .conform(
                Uuid::new_v4(),
                json!({"adapterId": "memcrm", "kind": "party", "limit": 25}),
            )
            .await;
        assert!(cross_tenant_conformance.is_err());

        let redeployed = execute_action(
            &store,
            runtime.executor.as_ref(),
            &governor,
            tenant,
            "deploy_adapter",
            Reversal::Compensating,
            payload,
        )
        .await?;
        assert_eq!(redeployed.state, EnvelopeState::Executed);
        let after_redeploy = store
            .bridge_adapters
            .get(tenant, "memcrm")
            .await?
            .expect("adapter remains registered");
        assert_eq!(after_redeploy.revision, record.revision);

        // Test the persisted non-incremental descriptor branch without
        // changing the checked-in fixture component.
        sqlx::query(
            "UPDATE bridge_adapter SET descriptor = jsonb_set(descriptor, '{capabilities,incremental_sync}', 'false'::jsonb) WHERE tenant_id = $1 AND adapter_id = $2",
        )
        .bind(tenant)
        .bind("memcrm")
        .execute(&db.pool)
        .await?;

        let synced = execute_action(
            &store,
            runtime.executor.as_ref(),
            &governor,
            tenant,
            "sync_adapter",
            Reversal::Compensating,
            json!({ "adapter_id": "memcrm", "kind": "party", "limit": 50 }),
        )
        .await?;
        assert_eq!(synced.state, EnvelopeState::Executed);
        let sync_receipt = store
            .execution_receipts
            .get_for_envelope(tenant, synced.id)
            .await?;
        assert_eq!(sync_receipt.details["strategy"], "full_relist");
        assert_eq!(sync_receipt.details["mode_details"]["page_count"], 1);
        assert!(!sync_receipt.details.to_string().contains("display_name"));
        let sync_state = store
            .bridge_sync
            .current(tenant, "memcrm", "party")
            .await?
            .expect("sync handler creates durable state");
        assert_eq!(sync_state.cursor, "");

        let paused = execute_action(
            &store,
            runtime.executor.as_ref(),
            &governor,
            tenant,
            "pause_adapter",
            Reversal::Snapshot,
            json!({ "adapter_id": "memcrm" }),
        )
        .await?;
        assert_eq!(paused.state, EnvelopeState::Executed);
        assert_eq!(
            store
                .bridge_adapters
                .get(tenant, "memcrm")
                .await?
                .expect("paused adapter exists")
                .state,
            BridgeAdapterState::Paused
        );

        let resumed = execute_action(
            &store,
            runtime.executor.as_ref(),
            &governor,
            tenant,
            "resume_adapter",
            Reversal::Snapshot,
            json!({ "adapter_id": "memcrm" }),
        )
        .await?;
        assert_eq!(resumed.state, EnvelopeState::Executed);
        assert_eq!(
            store
                .bridge_adapters
                .get(tenant, "memcrm")
                .await?
                .expect("resumed adapter exists")
                .state,
            BridgeAdapterState::Active
        );

        let other_tenant = Uuid::new_v4();
        let cross_tenant = execute_action(
            &store,
            runtime.executor.as_ref(),
            &governor,
            other_tenant,
            "pause_adapter",
            Reversal::Snapshot,
            json!({ "adapter_id": "memcrm" }),
        )
        .await;
        assert!(cross_tenant.is_err());
        assert!(store
            .bridge_adapters
            .get(other_tenant, "memcrm")
            .await?
            .is_none());
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn configured_runtime_rejects_probe_only_adapter_before_active(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let adapters_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters");
        let (runtime, _worker) = RuntimeServices::build_with_config_and_secrets(
            store.clone(),
            LlmRuntimeConfig {
                adapters_path: Some(adapters_path.to_string_lossy().into_owned()),
                egress_proxy_url: Some("http://127.0.0.1:9".to_owned()),
                ..LlmRuntimeConfig::default()
            },
            Arc::new(bridge_host::StaticSecretSource::default()),
            RuntimeAvailability::Available,
        )?;

        // Probe accepts the adapter-declared kind, but the WIT schema export
        // rejects it. Activation must fail closed before entering `active`.
        let error = execute_action(
            &store,
            runtime.executor.as_ref(),
            &bridge_governor(),
            tenant,
            "deploy_adapter",
            Reversal::Compensating,
            json!({
                "adapter_id": "memcrm",
                "wiring_ref": "memcrm.wasm",
                "grant": {
                    "origins": [],
                    "secret_names": [],
                    "dsn_name": null,
                    "fuel": 100_000
                },
                "config": {"kinds": ["unsupported"]}
            }),
        )
        .await
        .expect_err("probe-only adapter must not activate");
        assert!(error.to_string().contains("conformance"));

        let record = store
            .bridge_adapters
            .get(tenant, "memcrm")
            .await?
            .expect("failed activation remains durable");
        assert_eq!(record.state, BridgeAdapterState::Failed);
        assert_eq!(record.revision, 2);
        assert!(record
            .last_error
            .as_deref()
            .is_some_and(|value| !value.is_empty() && value.len() <= 512));
        assert!(!record
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("customer"));
        let history = store.bridge_adapters.history(tenant, "memcrm").await?;
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].event["event"], "registered");
        assert_eq!(history[2].to_state, BridgeAdapterState::Failed);
        assert_eq!(history[2].event["event"], "activation_conformance_failed");
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn configured_runtime_fails_closed_when_bridge_egress_client_is_invalid(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let adapters_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters");
        let (runtime, _worker) = RuntimeServices::build_with_config_and_secrets(
            store,
            LlmRuntimeConfig {
                adapters_path: Some(adapters_path.to_string_lossy().into_owned()),
                egress_proxy_url: Some("http://user:secret@[invalid".to_owned()),
                ..LlmRuntimeConfig::default()
            },
            Arc::new(bridge_host::StaticSecretSource::default()),
            RuntimeAvailability::Available,
        )?;

        assert!(runtime.bridge_lifecycle.is_none());
        assert!(matches!(
            runtime.components.bridge_lifecycle,
            RuntimeAvailability::Unavailable(ref reason)
                if reason == "configured bridge egress client is unavailable"
        ));
        assert!(!runtime
            .execution_registry
            .runtime_capabilities()
            .contains(fabric::capabilities::RUNTIME_BRIDGE_DEPLOY_ADAPTER));
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

async fn execute_action(
    store: &Store,
    executor: &hydra_kernel::executor::Executor,
    governor: &Governor,
    tenant: Uuid,
    action: &str,
    reversal: Reversal,
    payload: serde_json::Value,
) -> Result<ActionEnvelope, Box<dyn std::error::Error>> {
    let envelope = ActionEnvelope {
        id: Uuid::new_v4(),
        tenant,
        domain: "bridges".to_owned(),
        action: action.to_owned(),
        kind: None,
        targets: vec![Uuid::new_v4()],
        payload,
        rationale: format!("bridge lifecycle integration action {action}"),
        reversal,
        blast: BlastRadius {
            entities: 1,
            ..BlastRadius::default()
        },
        invocation: governor::InvocationContext::default(),
        state: EnvelopeState::Proposed,
        history: Vec::new(),
    };
    let token = match governor.evaluate(
        &envelope,
        &SpendSnapshot {
            month_to_date_cents: 0,
        },
    ) {
        governor::Decision::Execute(token) => token,
        decision => return Err(format!("expected execute decision, got {decision:?}").into()),
    };
    store.envelopes.save(tenant, &envelope).await?;
    store
        .envelopes
        .transition(
            tenant,
            envelope.id,
            EnvelopeState::Approved,
            "governor",
            &FixedClock,
        )
        .await?;
    Ok(executor.execute(token, &FixedClock).await?)
}

fn bridge_governor() -> Governor {
    let mut matrix = PolicyMatrix::default();
    for action in [
        "deploy_adapter",
        "pause_adapter",
        "resume_adapter",
        "sync_adapter",
    ] {
        matrix
            .insert(
                "bridges",
                Some(action),
                None,
                Cell {
                    level: Level::L4,
                    batch_max: Some(1),
                },
            )
            .expect("bridge policy cell should be unique");
    }
    Governor {
        matrix,
        constitution: Constitution {
            monthly_spend_cap_cents: 50_000,
            pii_egress_allowlist: vec!["private".to_owned()],
            blast_entities_ceiling: 250,
            blast_sends_ceiling: 50,
            blast_money_ceiling_cents: 250_000,
        },
    }
}
