use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Extension, Path, State};
use axum::Json;
use fabric::capabilities::{CapabilityRegistry, RUNTIME_BRIDGE_SYNC_ADAPTER};
use fabric::services::{
    AppState, ConciergeServiceImpl, StoreAutonomyService, StoreEntityService, StoreEnvelopeService,
    StoreTkStatsService,
};
use fabric::{
    AuthCtx, BridgeService, CorrelationContext, PrincipalContext, PrincipalType, Role, Scope,
    Session, SessionStore, StoreBridgeService,
};
use governor::{Cell, Constitution, Governor, Level, PolicyMatrix};
use serde_json::json;
use store::{BridgeAdapterState, BridgeAdapterTransition, NewBridgeAdapter, Store, TestDb};
use uuid::Uuid;

struct FixedGovernorProvider {
    governor: Arc<Governor>,
}

#[async_trait]
impl fabric::GovernorProvider for FixedGovernorProvider {
    async fn governor(&self, _tenant: Uuid) -> Result<Arc<Governor>, fabric::FabricError> {
        Ok(self.governor.clone())
    }
}

#[tokio::test]
async fn bridge_status_and_lifecycle_requests_use_durable_registry_and_envelopes(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let record = store
            .bridge_adapters
            .create(NewBridgeAdapter {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                component_ref: "memcrm.wasm".to_owned(),
                component_sha256: "a".repeat(64),
                grant_config: json!({
                    "origins": [],
                    "secret_names": [],
                    "dsn_name": null,
                    "fuel": 100_000
                }),
                config: json!({}),
            })
            .await?;
        let _record = store
            .bridge_adapters
            .transition(BridgeAdapterTransition {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                expected_revision: record.revision,
                expected_state: BridgeAdapterState::Inactive,
                new_state: BridgeAdapterState::Active,
                descriptor: Some(json!({ "name": "memcrm", "version": "1" })),
                last_error: None,
                event: json!({ "event": "test_activated" }),
            })
            .await?;

        let service = StoreBridgeService::with_governor_provider(
            store.clone(),
            Arc::new(FixedGovernorProvider {
                governor: Arc::new(bridge_governor()),
            }),
        );
        let ctx = admin_ctx(tenant);
        let status = service.status(tenant, "memcrm").await?;
        assert_eq!(status.state, "active");
        assert_eq!(status.wiring_ref.as_deref(), Some("memcrm.wasm"));
        assert!(status.envelope_id.is_none());

        let paused = service.pause(&ctx, tenant, "user:admin", "memcrm").await?;
        assert_eq!(paused.state, "queued");
        let pause_id = paused.envelope_id.expect("pause proposal envelope");
        let pause_envelope = store.envelopes.get(tenant, pause_id).await?;
        assert_eq!(pause_envelope.action, "pause_adapter");
        assert_eq!(
            store
                .adapter_kv
                .get_for_tenant(tenant, "memcrm", "paused")
                .await?,
            None
        );
        assert_eq!(
            store
                .bridge_adapters
                .get(tenant, "memcrm")
                .await?
                .expect("registry record remains authoritative")
                .state,
            BridgeAdapterState::Active
        );

        let paused_record = store
            .bridge_adapters
            .get(tenant, "memcrm")
            .await?
            .expect("adapter record exists");
        store
            .bridge_adapters
            .transition(BridgeAdapterTransition {
                tenant_id: tenant,
                adapter_id: "memcrm".to_owned(),
                expected_revision: paused_record.revision,
                expected_state: BridgeAdapterState::Active,
                new_state: BridgeAdapterState::Paused,
                descriptor: None,
                last_error: None,
                event: json!({ "event": "test_paused" }),
            })
            .await?;
        let resumed = service.resume(&ctx, tenant, "user:admin", "memcrm").await?;
        assert_eq!(resumed.state, "queued");
        let resume_id = resumed.envelope_id.ok_or("resume proposal envelope")?;
        assert_eq!(
            store.envelopes.get(tenant, resume_id).await?.action,
            "resume_adapter"
        );

        let other_tenant = Uuid::new_v4();
        assert!(matches!(
            service.status(other_tenant, "memcrm").await,
            Err(fabric::FabricError::NotFound)
        ));
        assert!(matches!(
            service
                .pause(&ctx, other_tenant, "user:admin", "memcrm")
                .await,
            Err(fabric::FabricError::NotFound)
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn nexus_bridge_sync_rest_is_path_bound_and_idempotent(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let capabilities =
            CapabilityRegistry::nexus_v1_with_runtime_capabilities([RUNTIME_BRIDGE_SYNC_ADAPTER])?;
        let state = AppState::new(
            Arc::new(SessionStore::new(store.pool.clone())),
            Arc::new(StoreEntityService::new(store.clone())),
            Arc::new(StoreAutonomyService::new(store.clone())),
            Arc::new(StoreBridgeService::new(store.clone(), bridge_governor())),
            Arc::new(StoreEnvelopeService::new(store.clone(), bridge_governor())),
            Arc::new(StoreTkStatsService::new(store.ledger.clone(), Vec::new())),
            Arc::new(ConciergeServiceImpl),
        )
        .with_capabilities(Arc::new(capabilities));
        let principal = nexus_principal(tenant);
        let body = json!({
            "kind": "party",
            "limit": 25,
            "rationale": "reconcile the next bounded bridge page",
            "idempotency_key": "sync-page-001"
        });

        let first = fabric::rest::propose_bridge_sync(
            State(state.clone()),
            Extension(principal.clone()),
            Path("memcrm".to_owned()),
            Json(body.clone()),
        )
        .await?
        .0;
        let second = fabric::rest::propose_bridge_sync(
            State(state.clone()),
            Extension(principal.clone()),
            Path("memcrm".to_owned()),
            Json(body),
        )
        .await?
        .0;
        assert_eq!(first["envelope_id"], second["envelope_id"]);
        assert_eq!(first["state"], "approved");

        let envelope_id = Uuid::parse_str(first["envelope_id"].as_str().expect("envelope id"))?;
        let envelope = store.envelopes.get(tenant, envelope_id).await?;
        assert_eq!(envelope.action, "sync_adapter");
        assert_eq!(envelope.payload["adapter_id"], "memcrm");
        assert_eq!(envelope.payload["kind"], "party");
        assert_eq!(envelope.payload["limit"], 25);

        let caller_selected_adapter = json!({
            "adapter_id": "other",
            "kind": "party",
            "limit": 25,
            "rationale": "must fail closed",
            "idempotency_key": "sync-page-002"
        });
        let error = fabric::rest::propose_bridge_sync(
            State(state.clone()),
            Extension(principal.clone()),
            Path("memcrm".to_owned()),
            Json(caller_selected_adapter),
        )
        .await
        .expect_err("adapter identity must remain path-bound");
        assert!(matches!(error, fabric::FabricError::ValidationFailed(_)));

        let caller_selected_tenant = json!({
            "hydra_tenant_id": Uuid::new_v4(),
            "kind": "party",
            "limit": 25,
            "rationale": "must fail closed",
            "idempotency_key": "sync-page-003"
        });
        let error = fabric::rest::propose_bridge_sync(
            State(state),
            Extension(principal),
            Path("memcrm".to_owned()),
            Json(caller_selected_tenant),
        )
        .await
        .expect_err("tenant authority must not come from the request body");
        assert!(matches!(error, fabric::FabricError::ValidationFailed(_)));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

fn admin_ctx(tenant: Uuid) -> AuthCtx {
    AuthCtx {
        principal: "user:admin".to_owned(),
        tenant,
        session: Some(Session {
            user_id: Uuid::new_v4(),
            tenant_id: tenant,
            username: "admin".to_owned(),
            roles: vec![Role::Admin],
            token: "fixture-session".to_owned(),
        }),
    }
}

fn nexus_principal(tenant: Uuid) -> PrincipalContext {
    PrincipalContext {
        principal_id: "nexus-service:bridge-sync-test".to_owned(),
        principal_type: PrincipalType::NexusService,
        hydra_tenant_id: tenant,
        external_provider: Some("nexus".to_owned()),
        external_tenant_id: Some("external-tenant".to_owned()),
        external_business_id: Some("external-business".to_owned()),
        scopes: BTreeSet::from([Scope::BridgesAdmin]),
        delegated_by: None,
        authentication_strength: Some("service".to_owned()),
        token_id: Some("token-bridge-sync".to_owned()),
        correlation: CorrelationContext {
            request_id: "request-bridge-sync".to_owned(),
            correlation_id: "correlation-bridge-sync".to_owned(),
            causation_id: Some("cause-bridge-sync".to_owned()),
        },
        binding_id: Some(Uuid::new_v4()),
        trace: store::TraceContext::fresh(),
    }
}

fn bridge_governor() -> Governor {
    let mut matrix = PolicyMatrix::default();
    for action in ["pause_adapter", "resume_adapter", "sync_adapter"] {
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
