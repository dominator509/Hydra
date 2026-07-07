use cdm::Entity;
use serde_json::json;
use store::{Store, StoreError, TestDb};
use uuid::Uuid;

#[tokio::test]
async fn regress_entities_upsert_writes_entity_event_and_outbox(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        let entity = party_entity(tenant, 1, "Ada Lovelace");

        let stored = store.entities.upsert(tenant, entity.clone()).await?;
        assert_eq!(stored, entity);

        let entity_count = sqlx::query!(r#"SELECT COUNT(*) as "count!: i64" FROM entity"#)
            .fetch_one(&db.pool)
            .await?
            .count;
        let event_count = sqlx::query!(
            r#"SELECT COUNT(*) as "count!: i64" FROM event_log WHERE tenant_id = $1"#,
            tenant
        )
        .fetch_one(&db.pool)
        .await?
        .count;
        let outbox_count = sqlx::query!(
            r#"SELECT COUNT(*) as "count!: i64" FROM outbox WHERE published_at IS NULL"#
        )
        .fetch_one(&db.pool)
        .await?
        .count;

        assert_eq!(entity_count, 1);
        assert_eq!(event_count, 1);
        assert_eq!(outbox_count, 1);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn regress_entities_detect_version_conflicts_across_pooled_connections(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let tenant = Uuid::new_v4();
        let store_a = Store::new(db.pool.clone());
        let store_b = Store::new(db.pool.clone());
        let seed = party_entity(tenant, 1, "Ada Lovelace");

        store_a.entities.upsert(tenant, seed.clone()).await?;

        let update_a = party_entity_with_id(seed.id, tenant, 2, "Ada A.");
        let update_b = party_entity_with_id(seed.id, tenant, 2, "Ada B.");
        let (left, right) = tokio::join!(
            store_a.entities.upsert(tenant, update_a),
            store_b.entities.upsert(tenant, update_b)
        );

        let (ok, err) = match (left, right) {
            (Ok(entity), Err(error)) => (entity, error),
            (Err(error), Ok(entity)) => (entity, error),
            other => panic!("expected exactly one success and one conflict, got {other:?}"),
        };

        assert_eq!(ok.version, 2);
        assert!(matches!(err, StoreError::Conflict(2)));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn regress_entities_cross_tenant_reads_return_not_found(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let entity = party_entity(tenant_a, 1, "Ada Lovelace");

        store.entities.upsert(tenant_a, entity.clone()).await?;
        let error = store
            .entities
            .get(tenant_b, entity.id)
            .await
            .expect_err("cross-tenant reads must miss");

        assert!(matches!(error, StoreError::NotFound));
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn regress_event_log_is_append_only() -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let tenant = Uuid::new_v4();
        let store = Store::new(db.pool.clone());
        let entity = party_entity(tenant, 1, "Ada Lovelace");

        store.entities.upsert(tenant, entity).await?;
        let seq = sqlx::query!(r#"SELECT seq FROM event_log LIMIT 1"#)
            .fetch_one(&db.pool)
            .await?
            .seq;

        let error = sqlx::query!(
            r#"
            UPDATE event_log
            SET kind = 'tamper'
            WHERE seq = $1
            "#,
            seq
        )
        .execute(&db.pool)
        .await
        .expect_err("event_log updates must be rejected");

        let message = error.to_string();
        assert!(message.contains("append-only"));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn regress_failed_outbox_write_rolls_back_entity_and_event(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let tenant = Uuid::new_v4();
        let entity = party_entity(tenant, 1, "Ada Lovelace");
        let mut tx = db.pool.begin().await?;

        sqlx::query!(
            r#"
            INSERT INTO entity (id, kind, tenant_id, body, origin, origin_ref, version)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            entity.id,
            entity.kind,
            tenant,
            entity.body.clone(),
            entity.origin,
            entity.origin_ref,
            entity.version as i64
        )
        .execute(&mut *tx)
        .await?;

        store::EventsRepo::append(
            &mut tx,
            tenant,
            "integration",
            "entity.insert",
            &json!({ "entity_id": entity.id }),
        )
        .await?;

        let error = sqlx::query!(
            r#"
            INSERT INTO outbox (event)
            VALUES ($1)
            "#,
            None::<serde_json::Value>
        )
        .execute(&mut *tx)
        .await
        .expect_err("null outbox payload must fail");
        let message = error.to_string();
        assert!(message.contains("null value"));

        tx.rollback().await?;

        let entity_count = sqlx::query!(
            r#"SELECT COUNT(*) as "count!: i64" FROM entity WHERE tenant_id = $1 AND id = $2"#,
            tenant,
            entity.id
        )
        .fetch_one(&db.pool)
        .await?
        .count;
        let event_count = sqlx::query!(
            r#"SELECT COUNT(*) as "count!: i64" FROM event_log WHERE tenant_id = $1"#,
            tenant
        )
        .fetch_one(&db.pool)
        .await?
        .count;

        assert_eq!(entity_count, 0);
        assert_eq!(event_count, 0);

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

fn party_entity(tenant: Uuid, version: u64, display_name: &str) -> Entity {
    party_entity_with_id(Uuid::new_v4(), tenant, version, display_name)
}

fn party_entity_with_id(id: Uuid, tenant: Uuid, version: u64, display_name: &str) -> Entity {
    Entity {
        id,
        kind: "party".to_owned(),
        tenant,
        body: json!({ "display_name": display_name }),
        origin: "native".to_owned(),
        origin_ref: None,
        version,
    }
}
