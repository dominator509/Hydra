use governor::{Clock, EnvelopeState, InvocationContext, Reversal};
use serde_json::json;
use store::TestDb;
use time::OffsetDateTime;
use uuid::Uuid;

struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[tokio::test]
async fn approved_recovery_is_bounded_and_preserves_tenant_identity(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let store = store::Store::new(db.pool.clone());
        for tenant in [tenant_a, tenant_b] {
            let envelope = envelope(tenant);
            store.envelopes.save(tenant, &envelope).await?;
            store
                .envelopes
                .transition(
                    tenant,
                    envelope.id,
                    EnvelopeState::Approved,
                    "test",
                    &TestClock,
                )
                .await?;
        }

        let recovered = store.envelopes.list_approved_all(1).await?;
        assert_eq!(recovered.len(), 1);
        assert!(recovered[0].tenant_id == tenant_a || recovered[0].tenant_id == tenant_b);

        let all = store.envelopes.list_approved_all(8).await?;
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|item| item.tenant_id == tenant_a));
        assert!(all.iter().any(|item| item.tenant_id == tenant_b));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

#[tokio::test]
async fn stale_executing_work_is_counted_without_mutation() -> Result<(), Box<dyn std::error::Error>>
{
    let db = TestDb::new().await?;
    let result = async {
        let tenant = Uuid::new_v4();
        let store = store::Store::new(db.pool.clone());
        let envelope = envelope(tenant);
        store.envelopes.save(tenant, &envelope).await?;
        store
            .envelopes
            .transition(
                tenant,
                envelope.id,
                EnvelopeState::Approved,
                "test",
                &TestClock,
            )
            .await?;
        store
            .envelopes
            .transition(
                tenant,
                envelope.id,
                EnvelopeState::Executing,
                "test",
                &TestClock,
            )
            .await?;
        sqlx::query(
            "UPDATE envelope SET updated_at = now() - interval '16 minutes' WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant)
        .bind(envelope.id)
        .execute(&db.pool)
        .await?;

        assert_eq!(store.envelopes.stale_executing_count().await?, 1);
        assert_eq!(store.envelopes.get(tenant, envelope.id).await?.state, EnvelopeState::Executing);
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}

fn envelope(tenant: Uuid) -> governor::ActionEnvelope {
    governor::ActionEnvelope {
        id: Uuid::new_v4(),
        tenant,
        domain: "pipeline".to_owned(),
        action: "move_stage".to_owned(),
        kind: Some("deal".to_owned()),
        targets: vec![Uuid::new_v4()],
        payload: json!({"stage": "won"}),
        rationale: "recovery test".to_owned(),
        reversal: Reversal::Compensating,
        blast: governor::BlastRadius {
            entities: 1,
            ..Default::default()
        },
        invocation: InvocationContext::default(),
        state: EnvelopeState::Proposed,
        history: Vec::new(),
    }
}
