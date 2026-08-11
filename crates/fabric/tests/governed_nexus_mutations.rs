use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::{Extension, State};
use axum::Json;
use cdm::Entity;
use fabric::capabilities::{RUNTIME_PIPELINE_MOVE_STAGE_DEAL, RUNTIME_TENANT_SCOPED_ENVELOPE_GET};
use fabric::rest::propose_stage_change;
use fabric::services::{
    demo_governor, AppState, ConciergeServiceImpl, StoreAutonomyService, StoreBridgeService,
    StoreEntityService, StoreEnvelopeService, StoreTkStatsService,
};
use fabric::{
    CapabilityRegistry, CorrelationContext, FabricError, PrincipalContext, PrincipalType, Scope,
    SessionStore,
};
use governor::{Cell, Constitution, Governor, Level, PolicyMatrix};
use serde_json::{json, Value};
use store::{Store, TestDb};
use uuid::Uuid;

#[tokio::test]
async fn governed_nexus_mutations_are_atomic_idempotent_and_do_not_write_entities(
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
                    body: json!({ "title": "Renewal", "stage_id": "discovery" }),
                    origin: "native".to_owned(),
                    origin_ref: None,
                    version: 1,
                },
            )
            .await?;
        let state = app_state(store.clone(), governor(Level::L4));
        let principal = principal(tenant, [Scope::CrmPropose]);
        let body = proposal_body(deal.id, "qualified", "stage-change-001");

        let (left, right) = tokio::join!(
            propose_stage_change(
                State(state.clone()),
                Extension(principal.clone()),
                Json(body.clone()),
            ),
            propose_stage_change(
                State(state.clone()),
                Extension(principal.clone()),
                Json(body.clone()),
            )
        );
        let left = left?.0;
        let right = right?.0;
        assert_eq!(left["envelope_id"], right["envelope_id"]);
        assert_eq!(left["state"], "approved");
        assert_eq!(left["decision"], "execute");

        let envelope_id = Uuid::parse_str(left["envelope_id"].as_str().expect("envelope id"))?;
        let envelope = store.envelopes.get(tenant, envelope_id).await?;
        assert_eq!(
            envelope.invocation.external_actor_id.as_deref(),
            Some("nexus-service:test")
        );
        assert_eq!(
            envelope.invocation.correlation_id.as_deref(),
            Some("corr-001")
        );
        assert_eq!(
            envelope.invocation.objective_id.as_deref(),
            Some("objective-001")
        );

        let envelope_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM envelope")
            .fetch_one(&db.pool)
            .await?;
        assert_eq!(
            envelope_count, 1,
            "concurrent retry must not create duplicates"
        );

        let unchanged = store.entities.get(tenant, deal.id).await?;
        assert_eq!(unchanged.body["stage_id"], "discovery");
        assert_eq!(
            unchanged.version, 1,
            "proposal must not mutate the CDM directly"
        );

        let conflict = propose_stage_change(
            State(state),
            Extension(principal),
            Json(proposal_body(deal.id, "won", "stage-change-001")),
        )
        .await
        .expect_err("same key with a different request must conflict");
        assert!(matches!(conflict, FabricError::IdempotencyConflict));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn governed_nexus_mutations_obey_current_autonomy_and_capability_authority(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let deal_id = Uuid::new_v4();
        let state = app_state(store.clone(), governor(Level::L2));
        let proposer = principal(tenant, [Scope::CrmPropose]);

        let queued = propose_stage_change(
            State(state.clone()),
            Extension(proposer.clone()),
            Json(proposal_body(deal_id, "qualified", "queue-001")),
        )
        .await?
        .0;
        assert_eq!(queued["state"], "pending_approval");
        assert_eq!(queued["decision"], "queue");

        let read_only = principal(tenant, [Scope::CrmRead]);
        let denied = propose_stage_change(
            State(state.clone()),
            Extension(read_only),
            Json(proposal_body(deal_id, "won", "denied-001")),
        )
        .await
        .expect_err("read-only principals cannot propose");
        assert!(matches!(denied, FabricError::AuthzDenied));

        let mut caller_selected_tenant = proposal_body(deal_id, "won", "tenant-001");
        caller_selected_tenant["hydra_tenant_id"] = json!(Uuid::new_v4());
        let denied = propose_stage_change(
            State(state),
            Extension(proposer.clone()),
            Json(caller_selected_tenant),
        )
        .await
        .expect_err("caller-supplied tenant authority must be rejected");
        assert!(matches!(denied, FabricError::ValidationFailed(_)));

        let unavailable_state =
            app_state_with_capabilities(store, governor(Level::L4), CapabilityRegistry::default());
        let unavailable = propose_stage_change(
            State(unavailable_state),
            Extension(proposer),
            Json(proposal_body(deal_id, "won", "unavailable-001")),
        )
        .await
        .expect_err("unregistered execution handlers must not be advertised");
        assert!(matches!(unavailable, FabricError::CapabilityUnavailable(_)));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

fn app_state(store: Store, governor: Governor) -> AppState {
    let capabilities = CapabilityRegistry::nexus_v1_with_runtime_capabilities([
        RUNTIME_PIPELINE_MOVE_STAGE_DEAL,
        RUNTIME_TENANT_SCOPED_ENVELOPE_GET,
    ])
    .expect("runtime capabilities are valid");
    app_state_with_capabilities(store, governor, capabilities)
}

fn app_state_with_capabilities(
    store: Store,
    governor: Governor,
    capabilities: CapabilityRegistry,
) -> AppState {
    AppState::new(
        Arc::new(SessionStore::new(store.pool.clone())),
        Arc::new(StoreEntityService::new(store.clone())),
        Arc::new(StoreAutonomyService::new(store.clone())),
        Arc::new(StoreBridgeService::new(store.clone(), demo_governor())),
        Arc::new(StoreEnvelopeService::new(store.clone(), governor)),
        Arc::new(StoreTkStatsService::new(store.ledger.clone(), Vec::new())),
        Arc::new(ConciergeServiceImpl),
    )
    .with_capabilities(Arc::new(capabilities))
}

fn governor(level: Level) -> Governor {
    let mut matrix = PolicyMatrix::default();
    matrix
        .insert(
            "pipeline",
            Some("move_stage"),
            Some("deal"),
            Cell {
                level,
                batch_max: Some(1),
            },
        )
        .expect("test policy is valid");
    Governor {
        matrix,
        constitution: Constitution {
            monthly_spend_cap_cents: 50_000,
            pii_egress_allowlist: vec!["private".to_owned()],
            blast_entities_ceiling: 10,
            blast_sends_ceiling: 0,
            blast_money_ceiling_cents: 0,
        },
    }
}

fn principal(tenant: Uuid, scopes: impl IntoIterator<Item = Scope>) -> PrincipalContext {
    PrincipalContext {
        principal_id: "nexus-service:test".to_owned(),
        principal_type: PrincipalType::NexusService,
        hydra_tenant_id: tenant,
        external_provider: Some("nexus".to_owned()),
        external_tenant_id: Some("external-tenant".to_owned()),
        external_business_id: Some("external-business".to_owned()),
        scopes: scopes.into_iter().collect::<BTreeSet<_>>(),
        delegated_by: None,
        authentication_strength: Some("service".to_owned()),
        token_id: Some("token-001".to_owned()),
        correlation: CorrelationContext {
            request_id: "req-001".to_owned(),
            correlation_id: "corr-001".to_owned(),
            causation_id: Some("cause-001".to_owned()),
        },
        binding_id: Some(Uuid::new_v4()),
        trace: store::TraceContext::fresh(),
    }
}

fn proposal_body(deal_id: Uuid, stage: &str, idempotency_key: &str) -> Value {
    json!({
        "deal_id": deal_id,
        "stage": stage,
        "rationale": "advance the canonical deal after qualification",
        "idempotency_key": idempotency_key,
        "objective_id": "objective-001",
        "task_id": "task-001"
    })
}
