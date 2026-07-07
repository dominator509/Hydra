use cdm::{Entity, KindRegistry};
use serde_json::{json, Value};
use sqlx::types::Json;
use sqlx::{PgPool, Transaction};
use uuid::Uuid;

use crate::{events::EventsRepo, StoreError};

#[derive(Clone)]
pub struct EntitiesRepo {
    pool: PgPool,
}

struct EntityRow {
    id: Uuid,
    kind: String,
    tenant_id: Uuid,
    body: Json<Value>,
    origin: String,
    origin_ref: Option<String>,
    version: i64,
}

impl EntitiesRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, tenant: Uuid, entity: Entity) -> Result<Entity, StoreError> {
        if entity.tenant != tenant {
            return Err(StoreError::TenantMismatch);
        }

        KindRegistry::default().validate(&entity.kind, &entity.body)?;

        let mut tx = self.pool.begin().await?;
        let previous_version = if entity.version == 0 {
            return Err(StoreError::Conflict(0));
        } else {
            entity.version - 1
        };

        let existing = sqlx::query!(
            r#"
            SELECT version
            FROM entity
            WHERE tenant_id = $1 AND id = $2
            FOR UPDATE
            "#,
            tenant,
            entity.id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let stored = match existing {
            Some(row) => {
                let current_version = as_u64(row.version)?;
                if current_version != previous_version {
                    return Err(StoreError::Conflict(current_version));
                }

                let row = sqlx::query_as!(
                    EntityRow,
                    r#"
                    UPDATE entity
                    SET kind = $3,
                        body = $4,
                        origin = $5,
                        origin_ref = $6,
                        version = $7,
                        deleted_at = NULL,
                        updated_at = now()
                    WHERE tenant_id = $1 AND id = $2
                    RETURNING
                        id,
                        kind,
                        tenant_id,
                        body as "body!: Json<Value>",
                        origin,
                        origin_ref,
                        version
                    "#,
                    tenant,
                    entity.id,
                    entity.kind,
                    entity.body.clone(),
                    entity.origin,
                    entity.origin_ref,
                    as_i64(entity.version)?
                )
                .fetch_one(&mut *tx)
                .await?;
                row_to_entity(row)?
            }
            None => {
                if entity.version != 1 {
                    return Err(StoreError::Conflict(0));
                }

                let row = sqlx::query_as!(
                    EntityRow,
                    r#"
                    INSERT INTO entity (id, kind, tenant_id, body, origin, origin_ref, version)
                    VALUES ($1, $2, $3, $4, $5, $6, $7)
                    RETURNING
                        id,
                        kind,
                        tenant_id,
                        body as "body!: Json<Value>",
                        origin,
                        origin_ref,
                        version
                    "#,
                    entity.id,
                    entity.kind,
                    tenant,
                    entity.body.clone(),
                    entity.origin,
                    entity.origin_ref,
                    as_i64(entity.version)?
                )
                .fetch_one(&mut *tx)
                .await?;
                row_to_entity(row)?
            }
        };

        let event_payload = json!({
            "entity_id": stored.id,
            "kind": stored.kind,
            "tenant_id": tenant,
            "version": stored.version,
            "origin": stored.origin,
            "origin_ref": stored.origin_ref,
        });
        EventsRepo::append(
            &mut tx,
            tenant,
            "store.entities",
            "entity.upsert",
            &event_payload,
        )
        .await?;
        insert_outbox(&mut tx, &event_payload).await?;

        tx.commit().await?;
        Ok(stored)
    }

    pub async fn get(&self, tenant: Uuid, id: Uuid) -> Result<Entity, StoreError> {
        let row = sqlx::query_as!(
            EntityRow,
            r#"
            SELECT
                id,
                kind,
                tenant_id,
                body as "body!: Json<Value>",
                origin,
                origin_ref,
                version
            FROM entity
            WHERE tenant_id = $1
              AND id = $2
              AND deleted_at IS NULL
            "#,
            tenant,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => row_to_entity(row),
            None => Err(StoreError::NotFound),
        }
    }

    pub async fn list(
        &self,
        tenant: Uuid,
        kind: &str,
        cursor: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<Entity>, StoreError> {
        let rows = sqlx::query_as!(
            EntityRow,
            r#"
            SELECT
                id,
                kind,
                tenant_id,
                body as "body!: Json<Value>",
                origin,
                origin_ref,
                version
            FROM entity
            WHERE tenant_id = $1
              AND kind = $2
              AND deleted_at IS NULL
              AND ($3::uuid IS NULL OR id > $3)
            ORDER BY id
            LIMIT $4
            "#,
            tenant,
            kind,
            cursor,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_entity).collect()
    }

    pub async fn soft_delete(&self, tenant: Uuid, id: Uuid) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query!(
            r#"
            UPDATE entity
            SET deleted_at = now(),
                version = version + 1,
                updated_at = now()
            WHERE tenant_id = $1
              AND id = $2
              AND deleted_at IS NULL
            RETURNING version
            "#,
            tenant,
            id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            return Err(StoreError::NotFound);
        };

        let event_payload = json!({
            "entity_id": id,
            "tenant_id": tenant,
            "version": as_u64(row.version)?,
            "deleted": true,
        });
        EventsRepo::append(
            &mut tx,
            tenant,
            "store.entities",
            "entity.soft_delete",
            &event_payload,
        )
        .await?;
        insert_outbox(&mut tx, &event_payload).await?;

        tx.commit().await?;
        Ok(())
    }
}

async fn insert_outbox(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    event: &Value,
) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        INSERT INTO outbox (event)
        VALUES ($1)
        "#,
        event.clone()
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn as_u64(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value)
        .map_err(|_| StoreError::Invariant(format!("negative version in database: {value}")))
}

fn as_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value)
        .map_err(|_| StoreError::Invariant(format!("version too large for database: {value}")))
}

fn row_to_entity(row: EntityRow) -> Result<Entity, StoreError> {
    Ok(Entity {
        id: row.id,
        kind: row.kind,
        tenant: row.tenant_id,
        body: row.body.0,
        origin: row.origin,
        origin_ref: row.origin_ref,
        version: as_u64(row.version)?,
    })
}
