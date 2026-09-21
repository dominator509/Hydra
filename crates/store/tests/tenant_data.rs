use cdm::Entity;
use serde_json::json;
use store::{LedgerRow, Store, StoreError, TestDb, MAX_EXPORT_RECORDS};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn party(tenant: Uuid, display_name: &str) -> Entity {
    Entity {
        id: Uuid::new_v4(),
        kind: "party".to_owned(),
        tenant,
        body: json!({"display_name": display_name}),
        origin: "native".to_owned(),
        origin_ref: None,
        version: 1,
    }
}

#[tokio::test]
async fn tenant_export_is_complete_for_bound_records_and_hides_other_tenants(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        let other_tenant = Uuid::new_v4();
        let entity = party(tenant, "Ada Lovelace");
        let other = party(other_tenant, "Other Tenant");

        store.entities.upsert(tenant, entity.clone()).await?;
        store.entities.upsert(other_tenant, other).await?;
        store
            .edges
            .upsert(cdm::Edge {
                src: entity.id,
                rel: "knows".to_owned(),
                dst: entity.id,
            })
            .await?;

        let export = store.tenant_data.export(tenant, 100).await?;
        assert_eq!(export.tenant_id, tenant);
        assert!(!export.truncated);
        assert_eq!(export.entities.len(), 1);
        assert_eq!(export.entities[0].id, entity.id);
        assert_eq!(export.edges.len(), 1);
        assert_eq!(export.events.len(), 1);
        assert_eq!(export.events[0].kind, "hydra.crm.entity.created.v1");
        assert!(serde_json::to_string(&export)?.contains("Ada Lovelace"));
        assert!(!serde_json::to_string(&export)?.contains("Other Tenant"));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn tenant_export_is_deterministically_bounded_and_soft_delete_remains_visible(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        let first = party(tenant, "First");
        let second = party(tenant, "Second");

        store.entities.upsert(tenant, first.clone()).await?;
        store.entities.upsert(tenant, second).await?;
        store.entities.soft_delete(tenant, first.id).await?;

        let bounded = store.tenant_data.export(tenant, 1).await?;
        assert!(bounded.truncated);
        assert_eq!(bounded.entities.len(), 1);
        let complete = store.tenant_data.export(tenant, 100).await?;
        let deleted = complete
            .entities
            .iter()
            .find(|candidate| candidate.id == first.id)
            .expect("soft-deleted entity remains exportable");
        assert!(deleted.deleted_at.is_some());

        let too_large = store
            .tenant_data
            .export(tenant, MAX_EXPORT_RECORDS + 1)
            .await
            .expect_err("oversized exports must fail closed");
        assert!(matches!(too_large, StoreError::Invariant(_)));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}

#[tokio::test]
async fn retention_preview_reports_candidates_without_mutating_state(
) -> Result<(), Box<dyn std::error::Error>> {
    let db = TestDb::new().await?;

    let result = async {
        let store = Store::new(db.pool.clone());
        let tenant = Uuid::new_v4();
        let entity = party(tenant, "Retained Subject");
        store.entities.upsert(tenant, entity.clone()).await?;
        store.entities.soft_delete(tenant, entity.id).await?;
        store
            .ledger
            .record(&LedgerRow {
                ts: OffsetDateTime::now_utc() - Duration::days(31),
                tenant_id: tenant,
                route: "test".to_owned(),
                provider: "fake".to_owned(),
                prefix_sha: "a".repeat(64),
                hit_tokens: 1,
                miss_tokens: 0,
                out_tokens: 1,
                out_bytes: 1,
                aborted: false,
                cost_cents: 0,
            })
            .await?;
        sqlx::query(
            "UPDATE entity SET deleted_at = now() - interval '31 days' WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant)
        .bind(entity.id)
        .execute(&db.pool)
        .await?;
        sqlx::query(
            "INSERT INTO event_log (tenant_id, ts, actor, kind, payload) VALUES ($1, now() - interval '31 days', 'test', 'test.old', '{}'::jsonb)",
        )
        .bind(tenant)
        .execute(&db.pool)
        .await?;

        let before_entity_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity")
            .fetch_one(&db.pool)
            .await?;
        let preview = store.tenant_data.retention_preview(tenant, 30).await?;
        let after_entity_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity")
            .fetch_one(&db.pool)
            .await?;

        assert_eq!(preview.tenant_id, tenant);
        assert_eq!(preview.age_days, 30);
        assert_eq!(preview.soft_deleted_entities.count, 1);
        assert!(preview.event_log.count >= 1);
        assert_eq!(preview.tokenkiller_ledger.count, 1);
        assert!(preview.pending_outbox_count >= 1);
        assert_eq!(before_entity_count, after_entity_count);

        let invalid = store
            .tenant_data
            .retention_preview(tenant, 0)
            .await
            .expect_err("zero-day preview must fail closed");
        assert!(matches!(invalid, StoreError::Invariant(_)));

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    db.cleanup().await?;
    result
}
