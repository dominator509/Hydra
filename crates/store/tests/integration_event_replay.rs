use cdm::Entity;
use serde_json::json;
use store::{Store, StoreError, TestDb};
use uuid::Uuid;

#[tokio::test]
async fn event_replay_selection_is_ordered_bounded_and_read_only(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;
    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        let entity_id = Uuid::new_v4();

        for version in 1..=3 {
            store
                .entities
                .upsert(
                    tenant,
                    Entity {
                        id: entity_id,
                        kind: "party".to_owned(),
                        tenant,
                        body: json!({"display_name": format!("Replay Party {version}")}),
                        origin: "native".to_owned(),
                        origin_ref: None,
                        version,
                    },
                )
                .await?;
        }

        let before: Vec<(i64, Option<time::OffsetDateTime>)> =
            sqlx::query_as("SELECT id, published_at FROM outbox ORDER BY id")
                .fetch_all(&db.pool)
                .await?;

        let first = store.outbox.list_for_replay(0, 2).await?;
        assert_eq!(first.len(), 2);
        assert!(first[0].id < first[1].id);
        assert_eq!(first[0].event_id, first[0].event.event_id);
        assert_eq!(first[0].subject, first[0].event.subject);

        let second = store.outbox.list_for_replay(first[1].id, 1000).await?;
        assert_eq!(second.len(), 1);
        assert!(second[0].id > first[1].id);

        let after: Vec<(i64, Option<time::OffsetDateTime>)> =
            sqlx::query_as("SELECT id, published_at FROM outbox ORDER BY id")
                .fetch_all(&db.pool)
                .await?;
        assert_eq!(before, after);

        assert!(matches!(
            store.outbox.list_for_replay(-1, 1).await,
            Err(StoreError::Invariant(_))
        ));
        assert!(matches!(
            store.outbox.list_for_replay(0, 1001).await,
            Err(StoreError::Invariant(_))
        ));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    db.cleanup().await?;
    result
}
