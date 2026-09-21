use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json;
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::StoreError;

pub const TENANT_DATA_SCHEMA_VERSION: &str = "hydra.tenant-data.v1";
pub const MAX_EXPORT_RECORDS: i64 = 10_000;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct TenantDataExport {
    pub schema_version: String,
    pub tenant_id: Uuid,
    pub truncated: bool,
    pub entities: Vec<ExportEntity>,
    pub edges: Vec<ExportEdge>,
    pub events: Vec<ExportEvent>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExportEntity {
    pub id: Uuid,
    pub kind: String,
    pub body: Value,
    pub origin: String,
    pub origin_ref: Option<String>,
    pub version: i64,
    pub deleted_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExportEdge {
    pub src: Uuid,
    pub rel: String,
    pub dst: Uuid,
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExportEvent {
    pub sequence: i64,
    pub event_id: Option<Uuid>,
    pub occurred_at: String,
    pub actor: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RetentionMetric {
    pub record_type: String,
    pub eligible_before: String,
    pub count: i64,
    pub oldest: Option<String>,
    pub newest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RetentionPreview {
    pub schema_version: String,
    pub tenant_id: Uuid,
    pub age_days: u16,
    pub generated_at: String,
    pub soft_deleted_entities: RetentionMetric,
    pub event_log: RetentionMetric,
    pub published_outbox: RetentionMetric,
    pub tokenkiller_ledger: RetentionMetric,
    pub pending_outbox_count: i64,
}

struct EntityExportRow {
    id: Uuid,
    kind: String,
    body: Json<Value>,
    origin: String,
    origin_ref: Option<String>,
    version: i64,
    deleted_at: Option<OffsetDateTime>,
    updated_at: OffsetDateTime,
}

struct EdgeExportRow {
    src: Uuid,
    rel: String,
    dst: Uuid,
    body: Json<Value>,
}

struct EventExportRow {
    sequence: i64,
    event_id: Option<Uuid>,
    occurred_at: OffsetDateTime,
    actor: String,
    kind: String,
}

struct RetentionRow {
    count: i64,
    oldest: Option<OffsetDateTime>,
    newest: Option<OffsetDateTime>,
}

#[derive(Clone)]
pub struct TenantDataRepo {
    pool: PgPool,
}

impl TenantDataRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn export(
        &self,
        tenant: Uuid,
        max_records: i64,
    ) -> Result<TenantDataExport, StoreError> {
        validate_limit(max_records)?;
        let query_limit = max_records + 1;

        let entities = sqlx::query_as!(
            EntityExportRow,
            r#"
            SELECT
                id,
                kind,
                body as "body!: Json<Value>",
                origin,
                origin_ref,
                version,
                deleted_at,
                updated_at
            FROM entity
            WHERE tenant_id = $1
            ORDER BY id
            LIMIT $2
            "#,
            tenant,
            query_limit,
        )
        .fetch_all(&self.pool)
        .await?;

        let edges = sqlx::query_as!(
            EdgeExportRow,
            r#"
            SELECT
                edge.src,
                edge.rel,
                edge.dst,
                edge.body as "body!: Json<Value>"
            FROM edge
            INNER JOIN entity AS src_entity
                ON src_entity.id = edge.src
               AND src_entity.tenant_id = $1
            INNER JOIN entity AS dst_entity
                ON dst_entity.id = edge.dst
               AND dst_entity.tenant_id = $1
            ORDER BY edge.src, edge.rel, edge.dst
            LIMIT $2
            "#,
            tenant,
            query_limit,
        )
        .fetch_all(&self.pool)
        .await?;

        let events = sqlx::query_as!(
            EventExportRow,
            r#"
            SELECT
                seq as sequence,
                event_id,
                ts as occurred_at,
                actor,
                kind
            FROM event_log
            WHERE tenant_id = $1
            ORDER BY seq
            LIMIT $2
            "#,
            tenant,
            query_limit,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut truncated = false;
        let entities = take_export_rows(entities, max_records, &mut truncated)
            .into_iter()
            .map(entity_export)
            .collect::<Result<Vec<_>, _>>()?;
        let edges = take_export_rows(edges, max_records, &mut truncated)
            .into_iter()
            .map(edge_export)
            .collect::<Vec<_>>();
        let events = take_export_rows(events, max_records, &mut truncated)
            .into_iter()
            .map(event_export)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TenantDataExport {
            schema_version: TENANT_DATA_SCHEMA_VERSION.to_owned(),
            tenant_id: tenant,
            truncated,
            entities,
            edges,
            events,
        })
    }

    pub async fn retention_preview(
        &self,
        tenant: Uuid,
        age_days: u16,
    ) -> Result<RetentionPreview, StoreError> {
        if age_days == 0 || age_days > 3_650 {
            return Err(StoreError::Invariant(
                "retention preview age_days must be between 1 and 3650".to_owned(),
            ));
        }

        let generated_at = OffsetDateTime::now_utc();
        let eligible_before = generated_at - Duration::days(i64::from(age_days));
        let tenant_text = tenant.to_string();

        let entities = sqlx::query_as!(
            RetentionRow,
            r#"
            SELECT
                COUNT(*) as "count!",
                MIN(deleted_at) as "oldest?",
                MAX(deleted_at) as "newest?"
            FROM entity
            WHERE tenant_id = $1
              AND deleted_at IS NOT NULL
              AND deleted_at <= $2
            "#,
            tenant,
            eligible_before,
        )
        .fetch_one(&self.pool)
        .await?;

        let events = sqlx::query_as!(
            RetentionRow,
            r#"
            SELECT
                COUNT(*) as "count!",
                MIN(ts) as "oldest?",
                MAX(ts) as "newest?"
            FROM event_log
            WHERE tenant_id = $1
              AND ts <= $2
            "#,
            tenant,
            eligible_before,
        )
        .fetch_one(&self.pool)
        .await?;

        let outbox = sqlx::query_as!(
            RetentionRow,
            r#"
            SELECT
                COUNT(*) as "count!",
                MIN(created_at) as "oldest?",
                MAX(created_at) as "newest?"
            FROM outbox
            WHERE published_at IS NOT NULL
              AND parked_at IS NULL
            AND event->>'hydra_tenant_id' = $1::text
              AND created_at <= $2
            "#,
            tenant_text,
            eligible_before,
        )
        .fetch_one(&self.pool)
        .await?;

        let ledger = sqlx::query_as!(
            RetentionRow,
            r#"
            SELECT
                COUNT(*) as "count!",
                MIN(ts) as "oldest?",
                MAX(ts) as "newest?"
            FROM tk_ledger
            WHERE tenant_id = $1
              AND ts <= $2
            "#,
            tenant,
            eligible_before,
        )
        .fetch_one(&self.pool)
        .await?;

        let pending_outbox = sqlx::query!(
            r#"
            SELECT COUNT(*) as "count!"
            FROM outbox
            WHERE published_at IS NULL
              AND parked_at IS NULL
              AND event->>'hydra_tenant_id' = $1::text
            "#,
            tenant_text,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(RetentionPreview {
            schema_version: TENANT_DATA_SCHEMA_VERSION.to_owned(),
            tenant_id: tenant,
            age_days,
            generated_at: format_timestamp(generated_at)?,
            soft_deleted_entities: metric("soft_deleted_entities", eligible_before, entities)?,
            event_log: metric("event_log", eligible_before, events)?,
            published_outbox: metric("published_outbox", eligible_before, outbox)?,
            tokenkiller_ledger: metric("tokenkiller_ledger", eligible_before, ledger)?,
            pending_outbox_count: pending_outbox.count,
        })
    }
}

fn validate_limit(limit: i64) -> Result<(), StoreError> {
    if (1..=MAX_EXPORT_RECORDS).contains(&limit) {
        Ok(())
    } else {
        Err(StoreError::Invariant(format!(
            "tenant export max_records must be between 1 and {MAX_EXPORT_RECORDS}"
        )))
    }
}

fn take_export_rows<T>(mut rows: Vec<T>, limit: i64, truncated: &mut bool) -> Vec<T> {
    if rows.len() > limit as usize {
        *truncated = true;
        rows.pop();
    }
    rows
}

fn entity_export(row: EntityExportRow) -> Result<ExportEntity, StoreError> {
    Ok(ExportEntity {
        id: row.id,
        kind: row.kind,
        body: row.body.0,
        origin: row.origin,
        origin_ref: row.origin_ref,
        version: row.version,
        deleted_at: row.deleted_at.map(format_timestamp).transpose()?,
        updated_at: format_timestamp(row.updated_at)?,
    })
}

fn edge_export(row: EdgeExportRow) -> ExportEdge {
    ExportEdge {
        src: row.src,
        rel: row.rel,
        dst: row.dst,
        body: row.body.0,
    }
}

fn event_export(row: EventExportRow) -> Result<ExportEvent, StoreError> {
    Ok(ExportEvent {
        sequence: row.sequence,
        event_id: row.event_id,
        occurred_at: format_timestamp(row.occurred_at)?,
        actor: row.actor,
        kind: row.kind,
    })
}

fn metric(
    record_type: &str,
    eligible_before: OffsetDateTime,
    row: RetentionRow,
) -> Result<RetentionMetric, StoreError> {
    Ok(RetentionMetric {
        record_type: record_type.to_owned(),
        eligible_before: format_timestamp(eligible_before)?,
        count: row.count,
        oldest: row.oldest.map(format_timestamp).transpose()?,
        newest: row.newest.map(format_timestamp).transpose()?,
    })
}

fn format_timestamp(value: OffsetDateTime) -> Result<String, StoreError> {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| StoreError::Invariant(format!("format tenant data timestamp: {error}")))
}
