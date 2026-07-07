#[path = "../src/executor.rs"]
mod executor;

use cdm::Entity;
use governor::{
    ActionEnvelope, BlastRadius, Cell, Clock, Constitution, Decision, EnvelopeState, Governor,
    Level, PolicyMatrix, Reversal, SpendSnapshot,
};
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

        let executor = executor::Executor::new(store.clone());
        let executed = executor.execute(token, &FixedClock).await?;
        assert_eq!(executed.state, EnvelopeState::Executed);

        let updated = store.entities.get(tenant, stored.id).await?;
        assert_eq!(updated.body["stage_id"], "won");
        assert_eq!(updated.version, 2);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
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
        constitution: Constitution {
            monthly_spend_cap_cents: 50_000,
            pii_egress_allowlist: vec!["private".into()],
            blast_entities_ceiling: 250,
            blast_sends_ceiling: 50,
            blast_money_ceiling_cents: 250_000,
        },
    }
}
