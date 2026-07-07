use governor::{ActionEnvelope, BlastRadius, Clock, EnvelopeState, Level, Reversal};
use serde_json::json;
use store::{Store, StoreError, TestDb};
use time::OffsetDateTime;
use uuid::Uuid;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }
}

#[tokio::test]
async fn regress_envelope_save_transition_and_list_round_trip(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let envelope = envelope(tenant);

        store.envelopes.save(tenant, &envelope).await?;
        let proposed = store
            .envelopes
            .list(tenant, EnvelopeState::Proposed)
            .await?;
        assert_eq!(proposed.len(), 1);
        assert_eq!(proposed[0].id, envelope.id);

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
        assert_eq!(approved.history.len(), 1);

        let approved_rows = sqlx::query!(
            r#"SELECT COUNT(*) as "count!: i64" FROM envelope_transition WHERE envelope_id = $1"#,
            envelope.id
        )
        .fetch_one(&db.pool)
        .await?
        .count;
        assert_eq!(approved_rows, 1);

        let approved_list = store
            .envelopes
            .list(tenant, EnvelopeState::Approved)
            .await?;
        assert_eq!(approved_list.len(), 1);
        assert_eq!(approved_list[0].history[0].actor, "governor");

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn regress_envelope_illegal_transition_is_rejected() -> Result<(), Box<dyn std::error::Error>>
{
    let db = TestDb::new().await?;

    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let envelope = envelope(tenant);

        store.envelopes.save(tenant, &envelope).await?;
        let error = store
            .envelopes
            .transition(
                tenant,
                envelope.id,
                EnvelopeState::Executing,
                "executor",
                &FixedClock,
            )
            .await
            .expect_err("proposed -> executing must fail");

        assert!(matches!(
            error,
            StoreError::Governor(governor::DomainError::IllegalTransition { .. })
        ));

        let transition_count =
            sqlx::query!(r#"SELECT COUNT(*) as "count!: i64" FROM envelope_transition"#)
                .fetch_one(&db.pool)
                .await?
                .count;
        assert_eq!(transition_count, 0);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn regress_autonomy_matrix_restores_specificity() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());

        store
            .autonomy
            .upsert_cell(
                tenant,
                "pipeline",
                "move_stage",
                None,
                Level::L2,
                &json!({ "batch_max": 5 }),
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
                &json!({ "batch_max": 2 }),
            )
            .await?;
        store
            .autonomy
            .upsert_cell(tenant, "pipeline", "archive", None, Level::L1, &json!({}))
            .await?;

        let matrix = store.autonomy.matrix(tenant).await?;
        let exact = matrix.resolve("pipeline", "move_stage", Some("deal"));
        let action = matrix.resolve("pipeline", "move_stage", Some("party"));
        let fallback = matrix.resolve("pipeline", "archive", Some("deal"));

        assert_eq!(exact.level, Level::L4);
        assert_eq!(exact.batch_max, Some(2));
        assert_eq!(action.level, Level::L2);
        assert_eq!(action.batch_max, Some(5));
        assert_eq!(fallback.level, Level::L1);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

fn envelope(tenant: Uuid) -> ActionEnvelope {
    ActionEnvelope {
        id: Uuid::new_v4(),
        tenant,
        domain: "pipeline".to_owned(),
        action: "move_stage".to_owned(),
        kind: Some("deal".to_owned()),
        targets: vec![Uuid::new_v4()],
        payload: json!({ "stage": "qualified" }),
        rationale: "integration test".to_owned(),
        reversal: Reversal::Compensating,
        blast: BlastRadius::default(),
        state: EnvelopeState::Proposed,
        history: Vec::new(),
    }
}
