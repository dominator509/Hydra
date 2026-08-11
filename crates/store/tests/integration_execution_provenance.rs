use governor::{ActionEnvelope, BlastRadius, Clock, EnvelopeState, InvocationContext, Reversal};
use serde_json::{json, Value};
use store::{
    ApprovalDecision, IdempotencyResolution, NewApprovalAssertion, NewIdempotencyRecord, Store,
    StoreError, TestDb,
};
use time::OffsetDateTime;
use uuid::Uuid;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }
}

#[test]
fn integration_execution_provenance_legacy_envelopes_default_invocation() {
    let tenant = Uuid::new_v4();
    let envelope = envelope(tenant, "nexus-agent");
    let mut legacy = serde_json::to_value(envelope).expect("serialize envelope");
    legacy
        .as_object_mut()
        .expect("envelope is an object")
        .remove("invocation");

    let decoded: ActionEnvelope = serde_json::from_value(legacy).expect("deserialize legacy");

    assert_eq!(decoded.invocation, InvocationContext::default());
}

#[tokio::test]
async fn integration_execution_provenance_idempotency_reuses_or_conflicts(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let envelope = envelope(tenant, "nexus-agent");
        store.envelopes.save(tenant, &envelope).await?;

        let request = NewIdempotencyRecord {
            tenant_id: tenant,
            origin_system: "nexus".to_owned(),
            idempotency_key: "stage-change-001".to_owned(),
            capability: "hydra.crm.propose_action".to_owned(),
            request_hash: "a".repeat(64),
            envelope_id: envelope.id,
        };
        let first = store.idempotency.record(request.clone()).await?;
        assert!(matches!(first, IdempotencyResolution::Recorded(_)));

        let repeated = store.idempotency.record(request.clone()).await?;
        assert!(matches!(repeated, IdempotencyResolution::Existing(_)));
        assert_eq!(repeated.record().envelope_id, envelope.id);

        let conflict = store
            .idempotency
            .record(NewIdempotencyRecord {
                request_hash: "b".repeat(64),
                ..request
            })
            .await
            .expect_err("different request hash must conflict");
        assert!(matches!(conflict, StoreError::IdempotencyConflict));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn integration_execution_provenance_approval_is_tenant_scoped_and_immutable(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let other_tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let envelope = envelope(tenant, "nexus-agent");
        store.envelopes.save(tenant, &envelope).await?;

        let cross_tenant = store
            .envelopes
            .get(other_tenant, envelope.id)
            .await
            .expect_err("cross-tenant lookup must fail closed");
        assert!(matches!(cross_tenant, StoreError::NotFound));

        let not_pending = store
            .approvals
            .create(approval(tenant, envelope.id, "human-approver"))
            .await
            .expect_err("only pending envelopes may be approved");
        assert!(matches!(not_pending, StoreError::ApprovalDenied));

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

        let own_proposal = store
            .approvals
            .create(approval(tenant, envelope.id, "nexus-agent"))
            .await
            .expect_err("proposer cannot approve its own action");
        assert!(matches!(own_proposal, StoreError::ApprovalDenied));

        let assertion = store
            .approvals
            .create(approval(tenant, envelope.id, "human-approver"))
            .await?;
        let latest = store
            .approvals
            .latest_approved(tenant, envelope.id)
            .await?
            .expect("approved assertion");
        assert_eq!(latest.id, assertion.id);
        assert_eq!(latest.correlation_id.as_deref(), Some("corr-001"));

        let mutation =
            sqlx::query("UPDATE approval_assertion SET comment = 'changed' WHERE id = $1")
                .bind(assertion.id)
                .execute(&db.pool)
                .await;
        assert!(mutation.is_err(), "approval assertions must be append-only");

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn integration_execution_provenance_transition_is_atomic_and_correlated(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let envelope = envelope(tenant, "nexus-agent");
        store.envelopes.save(tenant, &envelope).await?;

        let left = store.envelopes.clone();
        let right = store.envelopes.clone();
        let (left_result, right_result) = tokio::join!(
            left.transition(
                tenant,
                envelope.id,
                EnvelopeState::PendingApproval,
                "governor-a",
                &FixedClock,
            ),
            right.transition(
                tenant,
                envelope.id,
                EnvelopeState::PendingApproval,
                "governor-b",
                &FixedClock,
            )
        );
        let successes = usize::from(left_result.is_ok()) + usize::from(right_result.is_ok());
        assert_eq!(successes, 1, "exactly one competing transition may commit");
        let failure = left_result
            .err()
            .or_else(|| right_result.err())
            .expect("one failure");
        assert!(matches!(
            failure,
            StoreError::Governor(governor::DomainError::IllegalTransition { .. })
                | StoreError::Conflict(_)
        ));

        let event: Value = sqlx::query_scalar(
            "SELECT payload FROM event_log WHERE tenant_id = $1 AND kind = 'hydra.crm.envelope.queued.v1'",
        )
        .bind(tenant)
        .fetch_one(&db.pool)
        .await?;
        assert_eq!(event["spec_version"], "1.0");
        assert_eq!(event["schema_version"], "1.0");
        assert_eq!(event["event_type"], "hydra.crm.envelope.queued.v1");
        assert_eq!(event["envelope_id"], json!(envelope.id));
        assert_eq!(event["correlation_id"], "corr-001");
        assert_eq!(event["causation_id"], "cause-001");

        let outbox_event: Value =
            sqlx::query_scalar("SELECT event FROM outbox ORDER BY id DESC LIMIT 1")
                .fetch_one(&db.pool)
                .await?;
        assert_eq!(outbox_event, event);

        let transition_invocation: Value = sqlx::query_scalar(
            "SELECT invocation FROM envelope_transition WHERE tenant_id = $1 AND envelope_id = $2",
        )
        .bind(tenant)
        .bind(envelope.id)
        .fetch_one(&db.pool)
        .await?;
        assert_eq!(transition_invocation["request_id"], "req-001");

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

fn envelope(tenant: Uuid, external_actor_id: &str) -> ActionEnvelope {
    ActionEnvelope {
        id: Uuid::new_v4(),
        tenant,
        domain: "pipeline".to_owned(),
        action: "move_stage".to_owned(),
        kind: Some("deal".to_owned()),
        targets: vec![Uuid::new_v4()],
        payload: json!({ "stage": "qualified" }),
        rationale: "Nexus requested a governed stage change".to_owned(),
        reversal: Reversal::Compensating,
        blast: BlastRadius::default(),
        invocation: InvocationContext {
            request_id: Some("req-001".to_owned()),
            correlation_id: Some("corr-001".to_owned()),
            causation_id: Some("cause-001".to_owned()),
            origin_system: Some("nexus".to_owned()),
            external_actor_id: Some(external_actor_id.to_owned()),
            external_actor_type: Some("nexus_agent".to_owned()),
            external_binding_id: None,
            objective_id: Some("objective-001".to_owned()),
            task_id: Some("task-001".to_owned()),
            approval_id: None,
            idempotency_key: Some("stage-change-001".to_owned()),
        },
        state: EnvelopeState::Proposed,
        history: Vec::new(),
    }
}

fn approval(tenant_id: Uuid, envelope_id: Uuid, human_actor_id: &str) -> NewApprovalAssertion {
    NewApprovalAssertion {
        id: Uuid::new_v4(),
        tenant_id,
        envelope_id,
        human_actor_id: human_actor_id.to_owned(),
        delegated_by: "nexus-user-001".to_owned(),
        authentication_strength: "urn:nexus:aal2".to_owned(),
        request_id: Some("approval-req-001".to_owned()),
        correlation_id: Some("corr-001".to_owned()),
        objective_id: Some("objective-001".to_owned()),
        task_id: Some("task-001".to_owned()),
        decision: ApprovalDecision::Approved,
        comment: Some("approved after review".to_owned()),
    }
}
