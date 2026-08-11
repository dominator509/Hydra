use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use cdm::Entity;
use fabric::{
    AuthorizationService, BlastRadiusDto, CapabilityRegistry, CorrelationContext,
    EnvelopeApprovalDecision, EnvelopeApprovalRequest, EnvelopeCreateRequest, EnvelopeService,
    GovernedExternalProposal, GovernorProvider, PrincipalContext, PrincipalType, Scope,
    StoreEnvelopeService,
};
use governor::{
    ActionEnvelope, BlastRadius, Cell, Clock, Constitution, Decision, EnvelopeState, Governor,
    Level, PolicyMatrix, Reversal, SpendSnapshot,
};
use hydra_kernel::executor::Executor;
use hydra_kernel::policy_provider::PersistedGovernorProvider;
use hydra_kernel::runtime_services::RuntimeServices;
use serde_json::json;
use store::{Store, TestDb};
use time::OffsetDateTime;
use uuid::Uuid;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }
}

#[tokio::test]
async fn integration_executor_moves_a_deal_stage() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let deal = Entity {
            id: Uuid::new_v4(),
            kind: "deal".into(),
            tenant,
            body: json!({
                "title": "Renewal",
                "stage_id": "discovery"
            }),
            origin: "native".into(),
            origin_ref: None,
            version: 1,
        };
        let stored = store.entities.upsert(tenant, deal).await?;

        let envelope = ActionEnvelope {
            id: Uuid::new_v4(),
            tenant,
            domain: "pipeline".into(),
            action: "move_stage".into(),
            kind: Some("deal".into()),
            targets: vec![stored.id],
            payload: json!({ "stage": "won" }),
            rationale: "integration executor".into(),
            reversal: Reversal::Compensating,
            blast: BlastRadius::default(),
            invocation: governor::InvocationContext::default(),
            state: EnvelopeState::Proposed,
            history: Vec::new(),
        };
        let governor = governor();
        let token = match governor.evaluate(
            &envelope,
            &SpendSnapshot {
                month_to_date_cents: 0,
            },
        ) {
            Decision::Execute(token) => token,
            other => panic!("expected execute decision, got {other:?}"),
        };

        let _ = store.envelopes.save(tenant, &envelope).await?;
        let approved = store
            .envelopes
            .transition(
                tenant,
                envelope.id,
                EnvelopeState::Approved,
                "governor",
                &FixedClock,
            )
            .await?;
        assert_eq!(approved.state, EnvelopeState::Approved);

        let executor = Executor::new(store.clone());
        let executed = executor.execute(token, &FixedClock).await?;
        assert_eq!(executed.state, EnvelopeState::Executed);

        let updated = store.entities.get(tenant, stored.id).await?;
        assert_eq!(updated.body["stage_id"], "won");
        assert_eq!(updated.version, 2);
        let receipt = store
            .execution_receipts
            .get_for_envelope(tenant, envelope.id)
            .await?;
        assert_eq!(receipt.outcome, store::ExecutionOutcome::Verified);
        assert_eq!(receipt.affected_targets, vec![stored.id]);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn integration_executor_requires_distinct_human_approval_and_persists_receipt(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let deal = create_deal(&store, tenant, "Queued renewal").await?;
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

        let provider: Arc<dyn GovernorProvider> = Arc::new(PersistedGovernorProvider::new(
            store.autonomy.clone(),
            constitution(),
        ));
        let (runtime, worker) = RuntimeServices::build(store.clone())?;
        let authorization = Arc::new(AuthorizationService::new(["mfa".to_owned()]));
        let service = StoreEnvelopeService::with_governor_provider(store.clone(), provider)
            .with_execution_dispatcher(runtime.dispatcher.clone())
            .with_authorization(authorization);
        let mut runtime_capabilities = runtime.execution_registry.runtime_capabilities();
        runtime_capabilities
            .insert(fabric::capabilities::RUNTIME_TENANT_SCOPED_ENVELOPE_GET.to_owned());
        let capabilities =
            CapabilityRegistry::nexus_v1_with_runtime_capabilities(runtime_capabilities)?;
        let capability = capabilities
            .get("hydra.crm.propose_action")
            .ok_or("missing proposal capability")?
            .clone();
        let proposer = external_principal(
            tenant,
            "agent:revenue",
            PrincipalType::NexusAgent,
            BTreeSet::from([Scope::CrmPropose]),
        );
        let proposed = service
            .propose_external(
                &proposer,
                &capability,
                GovernedExternalProposal {
                    request: EnvelopeCreateRequest {
                        domain: "pipeline".to_owned(),
                        action: "move_stage".to_owned(),
                        kind: Some("deal".to_owned()),
                        targets: vec![deal.id],
                        payload: json!({"stage": "won"}),
                        rationale: "Nexus objective requests a stage change".to_owned(),
                        reversal: Reversal::Compensating,
                        blast: BlastRadiusDto {
                            entities: 1,
                            external_sends: 0,
                            money_cents: 0,
                            pii_egress: false,
                        },
                    },
                    idempotency_key: "stage-change-approval-1".to_owned(),
                    request_hash: "a".repeat(64),
                    objective_id: Some("objective-42".to_owned()),
                    task_id: Some("task-7".to_owned()),
                },
            )
            .await?;
        assert_eq!(proposed.state, EnvelopeState::PendingApproval);

        let mut agent_approver = external_principal(
            tenant,
            "agent:approver",
            PrincipalType::NexusAgent,
            BTreeSet::from([Scope::EnvelopesApprove]),
        );
        agent_approver.delegated_by = Some("human:owner".to_owned());
        agent_approver.authentication_strength = Some("mfa".to_owned());
        let denied = service
            .decide_external_approval(&agent_approver, proposed.id, approve_request())
            .await;
        assert!(matches!(denied, Err(fabric::FabricError::AuthzDenied)));

        let mut self_approver = external_principal(
            tenant,
            &proposer.principal_id,
            PrincipalType::Human,
            BTreeSet::from([Scope::EnvelopesApprove]),
        );
        self_approver.delegated_by = Some("nexus:user-session".to_owned());
        self_approver.authentication_strength = Some("mfa".to_owned());
        let denied = service
            .decide_external_approval(&self_approver, proposed.id, approve_request())
            .await;
        assert!(matches!(denied, Err(fabric::FabricError::AuthzDenied)));

        let mut human = external_principal(
            tenant,
            "human:finance-owner",
            PrincipalType::Human,
            BTreeSet::from([Scope::EnvelopesApprove]),
        );
        human.delegated_by = Some("nexus:user-session".to_owned());
        human.authentication_strength = Some("mfa".to_owned());
        human.correlation.request_id = "approval-request-1".to_owned();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let worker_handle = tokio::spawn(worker.run(runtime.executor.clone(), shutdown_rx));
        let approval = service
            .decide_external_approval(&human, proposed.id, approve_request())
            .await?;
        assert_eq!(approval.state, EnvelopeState::Approved);

        let executed = wait_for_state(&store, tenant, proposed.id, EnvelopeState::Executed).await?;
        assert_eq!(
            executed.invocation.approval_id,
            Some(approval.approval_id.to_string())
        );
        assert_eq!(
            executed.invocation.objective_id.as_deref(),
            Some("objective-42")
        );
        let assertion = store.approvals.get(tenant, approval.approval_id).await?;
        assert_eq!(assertion.human_actor_id, human.principal_id);
        assert_eq!(assertion.authentication_strength, "mfa");
        let receipt = store
            .execution_receipts
            .get_for_envelope(tenant, proposed.id)
            .await?;
        assert_eq!(receipt.outcome, store::ExecutionOutcome::Verified);
        assert_eq!(
            receipt.invocation.correlation_id,
            executed.invocation.correlation_id
        );
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
async fn integration_executor_rejects_missing_approval_assertion(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let deal = create_deal(&store, tenant, "Missing approval").await?;
        let envelope = stage_envelope(tenant, deal.id, "pipeline", "move_stage");
        let token = match governor().evaluate(
            &envelope,
            &SpendSnapshot {
                month_to_date_cents: 0,
            },
        ) {
            Decision::Execute(token) => token,
            other => panic!("expected execute token, got {other:?}"),
        };
        store.envelopes.save(tenant, &envelope).await?;
        store
            .envelopes
            .transition(
                tenant,
                envelope.id,
                EnvelopeState::PendingApproval,
                "governor",
                &FixedClock,
            )
            .await?;
        store
            .envelopes
            .transition(
                tenant,
                envelope.id,
                EnvelopeState::Approved,
                "forged-approver",
                &FixedClock,
            )
            .await?;

        let error = Executor::new(store.clone())
            .execute(token, &FixedClock)
            .await
            .expect_err("execution without immutable approval must fail");
        assert!(matches!(
            error,
            hydra_kernel::executor::ExecuteError::MissingApproval
        ));
        assert_eq!(
            store.envelopes.get(tenant, envelope.id).await?.state,
            EnvelopeState::Approved
        );
        assert!(matches!(
            store
                .execution_receipts
                .get_for_envelope(tenant, envelope.id)
                .await,
            Err(store::StoreError::NotFound)
        ));
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn integration_executor_rejects_unsupported_approved_envelope(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let deal = create_deal(&store, tenant, "Unsupported action").await?;
        let envelope = stage_envelope(tenant, deal.id, "pipeline", "unsupported_action");
        let mut matrix = PolicyMatrix::default();
        matrix.insert(
            "pipeline",
            Some("unsupported_action"),
            Some("deal"),
            Cell {
                level: Level::L4,
                batch_max: Some(1),
            },
        )?;
        let unsupported_governor = Governor {
            matrix,
            constitution: constitution(),
        };
        let token = match unsupported_governor.evaluate(
            &envelope,
            &SpendSnapshot {
                month_to_date_cents: 0,
            },
        ) {
            Decision::Execute(token) => token,
            other => panic!("expected execute token, got {other:?}"),
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
        let error = Executor::new(store.clone())
            .execute(token, &FixedClock)
            .await
            .expect_err("unsupported handler must fail closed");
        assert!(matches!(
            error,
            hydra_kernel::executor::ExecuteError::Registry(
                hydra_kernel::execution_registry::ExecutionRegistryError::UnsupportedEnvelope(_)
            )
        ));
        assert_eq!(
            store.envelopes.get(tenant, envelope.id).await?.state,
            EnvelopeState::Approved
        );
        assert_eq!(
            store.entities.get(tenant, deal.id).await?.body["stage_id"],
            "discovery"
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

async fn create_deal(
    store: &Store,
    tenant: Uuid,
    title: &str,
) -> Result<Entity, store::StoreError> {
    store
        .entities
        .upsert(
            tenant,
            Entity {
                id: Uuid::new_v4(),
                kind: "deal".to_owned(),
                tenant,
                body: json!({"title": title, "stage_id": "discovery"}),
                origin: "native".to_owned(),
                origin_ref: None,
                version: 1,
            },
        )
        .await
}

fn stage_envelope(tenant: Uuid, deal_id: Uuid, domain: &str, action: &str) -> ActionEnvelope {
    ActionEnvelope {
        id: Uuid::new_v4(),
        tenant,
        domain: domain.to_owned(),
        action: action.to_owned(),
        kind: Some("deal".to_owned()),
        targets: vec![deal_id],
        payload: json!({"stage": "won"}),
        rationale: "executor integration".to_owned(),
        reversal: Reversal::Compensating,
        blast: BlastRadius::default(),
        invocation: governor::InvocationContext::default(),
        state: EnvelopeState::Proposed,
        history: Vec::new(),
    }
}

fn approve_request() -> EnvelopeApprovalRequest {
    EnvelopeApprovalRequest {
        decision: EnvelopeApprovalDecision::Approve,
        comment: Some("Reviewed against the Nexus objective".to_owned()),
    }
}

fn external_principal(
    tenant: Uuid,
    principal_id: &str,
    principal_type: PrincipalType,
    scopes: BTreeSet<Scope>,
) -> PrincipalContext {
    PrincipalContext {
        principal_id: principal_id.to_owned(),
        principal_type,
        hydra_tenant_id: tenant,
        external_provider: Some("nexus".to_owned()),
        external_tenant_id: Some("nexus-tenant".to_owned()),
        external_business_id: Some("business-42".to_owned()),
        scopes,
        delegated_by: None,
        authentication_strength: None,
        token_id: Some(Uuid::new_v4().to_string()),
        correlation: CorrelationContext {
            request_id: "request-42".to_owned(),
            correlation_id: "correlation-42".to_owned(),
            causation_id: Some("causation-7".to_owned()),
        },
        binding_id: Some(Uuid::new_v4()),
        trace: store::TraceContext::fresh(),
    }
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

fn governor() -> Governor {
    let mut matrix = PolicyMatrix::default();
    matrix
        .insert(
            "pipeline",
            Some("move_stage"),
            Some("deal"),
            Cell {
                level: Level::L4,
                batch_max: Some(25),
            },
        )
        .expect("policy matrix insert should succeed");

    Governor {
        matrix,
        constitution: constitution(),
    }
}

fn constitution() -> Constitution {
    Constitution {
        monthly_spend_cap_cents: 50_000,
        pii_egress_allowlist: vec!["private".into()],
        blast_entities_ceiling: 250,
        blast_sends_ceiling: 50,
        blast_money_ceiling_cents: 250_000,
    }
}
