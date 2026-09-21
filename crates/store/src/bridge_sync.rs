use std::collections::BTreeMap;

use cdm::{
    builtin_kind_names, EventDataClass, EventEntityRef, HydraEventEnvelope, HydraEventPayload,
    HydraEventType, KindRegistry,
};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{events::canonical_now, events::EventsRepo, EventProvenance, StoreError};

const MAX_CURSOR: usize = 2048;
const MAX_EXTERNAL_ID: usize = 360;
const MAX_REASON: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeSyncOperation {
    Upsert,
    Delete,
}

impl BridgeSyncOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Upsert => "upserted",
            Self::Delete => "deleted",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BridgeSyncChange {
    pub operation: BridgeSyncOperation,
    pub kind: String,
    pub external_id: String,
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSyncState {
    pub tenant_id: Uuid,
    pub adapter_id: String,
    pub kind: String,
    pub cursor: String,
    pub revision: i64,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeSyncRunStatus {
    Running,
    Succeeded,
    Failed,
}

impl BridgeSyncRunStatus {
    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => Err(StoreError::Invariant(format!(
                "unknown bridge sync run status '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSyncRun {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub adapter_id: String,
    pub kind: String,
    pub start_cursor: String,
    pub next_cursor: Option<String>,
    pub status: BridgeSyncRunStatus,
    pub applied_upserts: i32,
    pub applied_deletes: i32,
    pub conflict_count: i32,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub envelope_id: Option<Uuid>,
    pub error: Option<String>,
    pub started_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBridgeSyncRun {
    pub tenant_id: Uuid,
    pub adapter_id: String,
    pub kind: String,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub envelope_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSyncConflict {
    pub operation: BridgeSyncOperation,
    pub kind: String,
    pub external_ref: String,
    pub conflict_kind: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSyncApply {
    pub applied_upserts: i32,
    pub applied_deletes: i32,
    pub next_cursor: String,
}

#[derive(Debug, Error)]
pub enum BridgeSyncApplyError {
    #[error("bridge sync conflict")]
    Conflict(BridgeSyncConflict),
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[derive(Clone)]
pub struct BridgeSyncRepo {
    pool: PgPool,
}

impl BridgeSyncRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn current(
        &self,
        tenant_id: Uuid,
        adapter_id: &str,
        kind: &str,
    ) -> Result<Option<BridgeSyncState>, StoreError> {
        validate_scope(tenant_id, adapter_id, kind)?;
        let row = sqlx::query_as!(
            BridgeSyncStateRow,
            r#"
            SELECT tenant_id, adapter_id, kind, cursor, revision, updated_at
            FROM bridge_sync_state
            WHERE tenant_id = $1 AND adapter_id = $2 AND kind = $3
            "#,
            tenant_id,
            adapter_id,
            kind,
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_state).transpose()
    }

    pub async fn start(&self, request: NewBridgeSyncRun) -> Result<BridgeSyncRun, StoreError> {
        validate_scope(request.tenant_id, &request.adapter_id, &request.kind)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query!(
            r#"
            INSERT INTO bridge_sync_state (tenant_id, adapter_id, kind)
            VALUES ($1, $2, $3)
            ON CONFLICT (tenant_id, adapter_id, kind) DO NOTHING
            "#,
            request.tenant_id,
            request.adapter_id,
            request.kind,
        )
        .execute(&mut *tx)
        .await?;

        let state = sqlx::query!(
            r#"
            SELECT cursor
            FROM bridge_sync_state
            WHERE tenant_id = $1 AND adapter_id = $2 AND kind = $3
            FOR UPDATE
            "#,
            request.tenant_id,
            request.adapter_id,
            request.kind,
        )
        .fetch_one(&mut *tx)
        .await?;

        let run_id = Uuid::new_v4();
        let row = sqlx::query_as!(
            BridgeSyncRunRow,
            r#"
            INSERT INTO bridge_sync_run (
                id, tenant_id, adapter_id, kind, start_cursor,
                correlation_id, causation_id, envelope_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING
                id, tenant_id, adapter_id, kind, start_cursor, next_cursor,
                status, applied_upserts, applied_deletes, conflict_count,
                correlation_id, causation_id, envelope_id, error, started_at,
                finished_at
            "#,
            run_id,
            request.tenant_id,
            request.adapter_id,
            request.kind,
            state.cursor,
            request.correlation_id,
            request.causation_id,
            request.envelope_id,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_active_run_error)?;

        tx.commit().await?;
        row_to_run(row)
    }

    pub async fn get_run(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
    ) -> Result<Option<BridgeSyncRun>, StoreError> {
        if tenant_id.is_nil() || run_id.is_nil() {
            return Err(StoreError::Invariant(
                "bridge sync run authority fields cannot be nil".to_owned(),
            ));
        }
        let row = sqlx::query_as!(
            BridgeSyncRunRow,
            r#"
            SELECT
                id, tenant_id, adapter_id, kind, start_cursor, next_cursor,
                status, applied_upserts, applied_deletes, conflict_count,
                correlation_id, causation_id, envelope_id, error, started_at,
                finished_at
            FROM bridge_sync_run
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant_id,
            run_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_run).transpose()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn apply_page(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        adapter_id: &str,
        kind: &str,
        changes: &[BridgeSyncChange],
        next_cursor: &str,
        provenance: &EventProvenance,
    ) -> Result<BridgeSyncApply, BridgeSyncApplyError> {
        validate_scope(tenant_id, adapter_id, kind)?;
        validate_cursor(next_cursor).map_err(BridgeSyncApplyError::Store)?;
        for change in changes {
            validate_change(kind, change).map_err(BridgeSyncApplyError::Conflict)?;
        }

        let mut tx = self.pool.begin().await.map_err(StoreError::from)?;
        let run = sqlx::query!(
            r#"
            SELECT id, start_cursor, status
            FROM bridge_sync_run
            WHERE tenant_id = $1 AND id = $2 AND adapter_id = $3 AND kind = $4
            FOR UPDATE
            "#,
            tenant_id,
            run_id,
            adapter_id,
            kind,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::from)?
        .ok_or(StoreError::NotFound)
        .map_err(BridgeSyncApplyError::Store)?;
        if run.status != "running" {
            return Err(BridgeSyncApplyError::Store(StoreError::Invariant(
                "bridge sync run is not active".to_owned(),
            )));
        }

        let state = sqlx::query!(
            r#"
            SELECT cursor
            FROM bridge_sync_state
            WHERE tenant_id = $1 AND adapter_id = $2 AND kind = $3
            FOR UPDATE
            "#,
            tenant_id,
            adapter_id,
            kind,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::from)?
        .ok_or(StoreError::NotFound)
        .map_err(BridgeSyncApplyError::Store)?;
        if state.cursor != run.start_cursor {
            return Err(BridgeSyncApplyError::Store(StoreError::Invariant(
                "bridge sync cursor changed while run was active".to_owned(),
            )));
        }

        let origin = format!("bridge:{adapter_id}");
        let mut applied_upserts = 0;
        let mut applied_deletes = 0;
        for change in changes {
            let origin_ref =
                origin_ref(kind, &change.external_id).map_err(BridgeSyncApplyError::Conflict)?;
            let existing = sqlx::query!(
                r#"
                SELECT id, version, deleted_at
                FROM entity
                WHERE tenant_id = $1 AND origin = $2 AND origin_ref = $3
                FOR UPDATE
                "#,
                tenant_id,
                origin,
                origin_ref,
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(StoreError::from)?;

            match change.operation {
                BridgeSyncOperation::Upsert => {
                    let (entity_id, version, is_created) = match existing {
                        Some(row) => {
                            let version =
                                next_version(row.version).map_err(BridgeSyncApplyError::Store)?;
                            sqlx::query!(
                                r#"
                                UPDATE entity
                                SET kind = $3, body = $4, version = $5,
                                    deleted_at = NULL, updated_at = now()
                                WHERE tenant_id = $1 AND id = $2
                                "#,
                                tenant_id,
                                row.id,
                                change.kind,
                                change.body.clone(),
                                version,
                            )
                            .execute(&mut *tx)
                            .await
                            .map_err(StoreError::from)?;
                            (row.id, version, false)
                        }
                        None => {
                            let entity_id = Uuid::new_v4();
                            sqlx::query!(
                                r#"
                                INSERT INTO entity (
                                    id, kind, tenant_id, body, origin, origin_ref, version
                                )
                                VALUES ($1, $2, $3, $4, $5, $6, 1)
                                "#,
                                entity_id,
                                change.kind,
                                tenant_id,
                                change.body.clone(),
                                origin,
                                origin_ref,
                            )
                            .execute(&mut *tx)
                            .await
                            .map_err(StoreError::from)?;
                            (entity_id, 1, true)
                        }
                    };
                    append_entity_event(
                        &mut tx,
                        tenant_id,
                        entity_id,
                        &change.kind,
                        &origin,
                        &origin_ref,
                        version,
                        if is_created {
                            HydraEventType::EntityCreated
                        } else {
                            HydraEventType::EntityUpdated
                        },
                        provenance,
                    )
                    .await
                    .map_err(BridgeSyncApplyError::Store)?;
                    applied_upserts += 1;
                }
                BridgeSyncOperation::Delete => {
                    let Some(row) = existing else {
                        return Err(BridgeSyncApplyError::Conflict(BridgeSyncConflict {
                            operation: change.operation,
                            kind: change.kind.clone(),
                            external_ref: change.external_id.clone(),
                            conflict_kind: "missing_delete_target".to_owned(),
                            reason: "bridge delete did not match a canonical entity".to_owned(),
                        }));
                    };
                    if row.deleted_at.is_some() {
                        applied_deletes += 1;
                        continue;
                    }
                    let version = next_version(row.version).map_err(BridgeSyncApplyError::Store)?;
                    sqlx::query!(
                        r#"
                        UPDATE entity
                        SET deleted_at = now(), version = $3, updated_at = now()
                        WHERE tenant_id = $1 AND id = $2
                        "#,
                        tenant_id,
                        row.id,
                        version,
                    )
                    .execute(&mut *tx)
                    .await
                    .map_err(StoreError::from)?;
                    append_entity_event(
                        &mut tx,
                        tenant_id,
                        row.id,
                        &change.kind,
                        &origin,
                        &origin_ref,
                        version,
                        HydraEventType::EntityDeleted,
                        provenance,
                    )
                    .await
                    .map_err(BridgeSyncApplyError::Store)?;
                    applied_deletes += 1;
                }
            }
        }

        sqlx::query!(
            r#"
            UPDATE bridge_sync_state
            SET cursor = $4, revision = revision + 1, updated_at = now()
            WHERE tenant_id = $1 AND adapter_id = $2 AND kind = $3
            "#,
            tenant_id,
            adapter_id,
            kind,
            next_cursor,
        )
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from)?;
        sqlx::query!(
            r#"
            UPDATE bridge_sync_run
            SET status = 'succeeded', next_cursor = $2,
                applied_upserts = $3, applied_deletes = $4,
                finished_at = now()
            WHERE tenant_id = $5 AND id = $1 AND status = 'running'
            "#,
            run_id,
            next_cursor,
            applied_upserts,
            applied_deletes,
            tenant_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from)?;
        tx.commit().await.map_err(StoreError::from)?;

        Ok(BridgeSyncApply {
            applied_upserts,
            applied_deletes,
            next_cursor: next_cursor.to_owned(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn apply_full_relist(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        adapter_id: &str,
        kind: &str,
        changes: &[BridgeSyncChange],
        provenance: &EventProvenance,
    ) -> Result<BridgeSyncApply, BridgeSyncApplyError> {
        validate_scope(tenant_id, adapter_id, kind).map_err(BridgeSyncApplyError::Store)?;
        let mut requested = BTreeMap::new();
        for change in changes {
            if change.operation != BridgeSyncOperation::Upsert {
                return Err(BridgeSyncApplyError::Conflict(BridgeSyncConflict {
                    operation: change.operation,
                    kind: change.kind.clone(),
                    external_ref: sanitize(&change.external_id, 512),
                    conflict_kind: "full_relist_delete_not_allowed".to_owned(),
                    reason: "full relist accepts only the complete upsert snapshot".to_owned(),
                }));
            }
            validate_change(kind, change).map_err(BridgeSyncApplyError::Conflict)?;
            let external_id = change.external_id.clone();
            if requested.insert(external_id.clone(), change).is_some() {
                return Err(BridgeSyncApplyError::Conflict(BridgeSyncConflict {
                    operation: change.operation,
                    kind: change.kind.clone(),
                    external_ref: sanitize(&external_id, 512),
                    conflict_kind: "duplicate_external_id".to_owned(),
                    reason: "full relist contains a duplicate external identity".to_owned(),
                }));
            }
        }

        let mut tx = self.pool.begin().await.map_err(StoreError::from)?;
        let run = sqlx::query!(
            r#"
            SELECT id, start_cursor, status
            FROM bridge_sync_run
            WHERE tenant_id = $1 AND id = $2 AND adapter_id = $3 AND kind = $4
            FOR UPDATE
            "#,
            tenant_id,
            run_id,
            adapter_id,
            kind,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::from)?
        .ok_or(StoreError::NotFound)
        .map_err(BridgeSyncApplyError::Store)?;
        if run.status != "running" {
            return Err(BridgeSyncApplyError::Store(StoreError::Invariant(
                "bridge sync run is not active".to_owned(),
            )));
        }

        let state = sqlx::query!(
            r#"
            SELECT cursor
            FROM bridge_sync_state
            WHERE tenant_id = $1 AND adapter_id = $2 AND kind = $3
            FOR UPDATE
            "#,
            tenant_id,
            adapter_id,
            kind,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::from)?
        .ok_or(StoreError::NotFound)
        .map_err(BridgeSyncApplyError::Store)?;
        if state.cursor != run.start_cursor {
            return Err(BridgeSyncApplyError::Store(StoreError::Invariant(
                "bridge sync cursor changed while run was active".to_owned(),
            )));
        }

        let origin = format!("bridge:{adapter_id}");
        let rows = sqlx::query_as::<_, FullRelistEntityRow>(
            r#"
            SELECT id, origin_ref, body, version, deleted_at
            FROM entity
            WHERE tenant_id = $1
              AND origin = $2
              AND kind = $3
              AND origin_ref IS NOT NULL
            FOR UPDATE
            "#,
        )
        .bind(tenant_id)
        .bind(&origin)
        .bind(kind)
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::from)?;
        let mut existing = rows
            .into_iter()
            .filter_map(|row| row.origin_ref.clone().map(|origin_ref| (origin_ref, row)))
            .collect::<BTreeMap<_, _>>();
        let mut applied_upserts = 0;
        let mut applied_deletes = 0;

        for change in requested.values() {
            let origin_ref =
                origin_ref(kind, &change.external_id).map_err(BridgeSyncApplyError::Conflict)?;
            let Some(row) = existing.remove(&origin_ref) else {
                let entity_id = Uuid::new_v4();
                sqlx::query(
                    r#"
                    INSERT INTO entity (
                        id, kind, tenant_id, body, origin, origin_ref, version
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, 1)
                    "#,
                )
                .bind(entity_id)
                .bind(&change.kind)
                .bind(tenant_id)
                .bind(change.body.clone())
                .bind(&origin)
                .bind(&origin_ref)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::from)?;
                append_entity_event(
                    &mut tx,
                    tenant_id,
                    entity_id,
                    &change.kind,
                    &origin,
                    &origin_ref,
                    1,
                    HydraEventType::EntityCreated,
                    provenance,
                )
                .await
                .map_err(BridgeSyncApplyError::Store)?;
                applied_upserts += 1;
                continue;
            };

            if row.deleted_at.is_none() && row.body == change.body {
                continue;
            }
            let version = next_version(row.version).map_err(BridgeSyncApplyError::Store)?;
            sqlx::query(
                r#"
                UPDATE entity
                SET kind = $3, body = $4, version = $5,
                    deleted_at = NULL, updated_at = now()
                WHERE tenant_id = $1 AND id = $2
                "#,
            )
            .bind(tenant_id)
            .bind(row.id)
            .bind(&change.kind)
            .bind(change.body.clone())
            .bind(version)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::from)?;
            append_entity_event(
                &mut tx,
                tenant_id,
                row.id,
                &change.kind,
                &origin,
                &origin_ref,
                version,
                HydraEventType::EntityUpdated,
                provenance,
            )
            .await
            .map_err(BridgeSyncApplyError::Store)?;
            applied_upserts += 1;
        }

        for (origin_ref, row) in existing {
            if row.deleted_at.is_some() {
                continue;
            }
            let version = next_version(row.version).map_err(BridgeSyncApplyError::Store)?;
            sqlx::query(
                r#"
                UPDATE entity
                SET deleted_at = now(), version = $3, updated_at = now()
                WHERE tenant_id = $1 AND id = $2
                "#,
            )
            .bind(tenant_id)
            .bind(row.id)
            .bind(version)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::from)?;
            append_entity_event(
                &mut tx,
                tenant_id,
                row.id,
                kind,
                &origin,
                &origin_ref,
                version,
                HydraEventType::EntityDeleted,
                provenance,
            )
            .await
            .map_err(BridgeSyncApplyError::Store)?;
            applied_deletes += 1;
        }

        sqlx::query(
            r#"
            UPDATE bridge_sync_state
            SET cursor = '', revision = revision + 1, updated_at = now()
            WHERE tenant_id = $1 AND adapter_id = $2 AND kind = $3
            "#,
        )
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(kind)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from)?;
        sqlx::query(
            r#"
            UPDATE bridge_sync_run
            SET status = 'succeeded', next_cursor = '',
                applied_upserts = $2, applied_deletes = $3,
                finished_at = now()
            WHERE tenant_id = $4 AND id = $1 AND status = 'running'
            "#,
        )
        .bind(run_id)
        .bind(applied_upserts)
        .bind(applied_deletes)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from)?;
        tx.commit().await.map_err(StoreError::from)?;

        Ok(BridgeSyncApply {
            applied_upserts,
            applied_deletes,
            next_cursor: String::new(),
        })
    }

    pub async fn fail(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        error: &str,
        conflict: Option<BridgeSyncConflict>,
        provenance: &EventProvenance,
    ) -> Result<(), StoreError> {
        if tenant_id.is_nil() || run_id.is_nil() {
            return Err(StoreError::Invariant(
                "bridge sync failure authority fields cannot be nil".to_owned(),
            ));
        }
        let error = sanitize(error, MAX_REASON);
        if error.is_empty() {
            return Err(StoreError::Invariant(
                "bridge sync failure reason cannot be empty".to_owned(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        let run = sqlx::query!(
            r#"
            SELECT adapter_id, kind, status
            FROM bridge_sync_run
            WHERE tenant_id = $1 AND id = $2
            FOR UPDATE
            "#,
            tenant_id,
            run_id,
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        if run.status != "running" {
            return Err(StoreError::Invariant(
                "bridge sync run is not active".to_owned(),
            ));
        }
        let conflict_count = i32::from(conflict.is_some());
        sqlx::query!(
            r#"
            UPDATE bridge_sync_run
            SET status = 'failed', error = $3, conflict_count = $4,
                finished_at = now()
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant_id,
            run_id,
            error,
            conflict_count,
        )
        .execute(&mut *tx)
        .await?;

        if let Some(conflict) = conflict {
            validate_conflict(&conflict)?;
            sqlx::query!(
                r#"
                INSERT INTO bridge_sync_conflict (
                    tenant_id, run_id, adapter_id, kind, external_ref,
                    operation, conflict_kind, reason
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
                tenant_id,
                run_id,
                run.adapter_id,
                run.kind,
                conflict.external_ref,
                conflict.operation.as_str(),
                conflict.conflict_kind,
                conflict.reason,
            )
            .execute(&mut *tx)
            .await?;
            let mut event = HydraEventEnvelope::new(
                Uuid::new_v4(),
                HydraEventType::SyncConflict,
                canonical_now()?,
                tenant_id,
                provenance.actor.clone(),
                EventDataClass::Private,
                HydraEventPayload::SyncConflict {
                    conflict_kind: conflict.conflict_kind,
                    bridge_id: Some(run.adapter_id.clone()),
                },
            );
            provenance.apply(&mut event);
            EventsRepo::append_canonical_with_trace(
                &mut tx,
                &event,
                provenance.trace_context.as_ref(),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

#[derive(Debug)]
struct BridgeSyncStateRow {
    tenant_id: Uuid,
    adapter_id: String,
    kind: String,
    cursor: String,
    revision: i64,
    updated_at: OffsetDateTime,
}

#[derive(Debug)]
struct BridgeSyncRunRow {
    id: Uuid,
    tenant_id: Uuid,
    adapter_id: String,
    kind: String,
    start_cursor: String,
    next_cursor: Option<String>,
    status: String,
    applied_upserts: i32,
    applied_deletes: i32,
    conflict_count: i32,
    correlation_id: Option<String>,
    causation_id: Option<String>,
    envelope_id: Option<Uuid>,
    error: Option<String>,
    started_at: OffsetDateTime,
    finished_at: Option<OffsetDateTime>,
}

#[derive(Debug)]
struct FullRelistEntityRow {
    id: Uuid,
    origin_ref: Option<String>,
    body: Value,
    version: i64,
    deleted_at: Option<OffsetDateTime>,
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for FullRelistEntityRow {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            origin_ref: row.try_get("origin_ref")?,
            body: row.try_get("body")?,
            version: row.try_get("version")?,
            deleted_at: row.try_get("deleted_at")?,
        })
    }
}

fn row_to_state(row: BridgeSyncStateRow) -> Result<BridgeSyncState, StoreError> {
    Ok(BridgeSyncState {
        tenant_id: row.tenant_id,
        adapter_id: row.adapter_id,
        kind: row.kind,
        cursor: row.cursor,
        revision: row.revision,
        updated_at: row.updated_at,
    })
}

fn row_to_run(row: BridgeSyncRunRow) -> Result<BridgeSyncRun, StoreError> {
    Ok(BridgeSyncRun {
        id: row.id,
        tenant_id: row.tenant_id,
        adapter_id: row.adapter_id,
        kind: row.kind,
        start_cursor: row.start_cursor,
        next_cursor: row.next_cursor,
        status: BridgeSyncRunStatus::parse(&row.status)?,
        applied_upserts: row.applied_upserts,
        applied_deletes: row.applied_deletes,
        conflict_count: row.conflict_count,
        correlation_id: row.correlation_id,
        causation_id: row.causation_id,
        envelope_id: row.envelope_id,
        error: row.error,
        started_at: row.started_at,
        finished_at: row.finished_at,
    })
}

fn validate_scope(tenant_id: Uuid, adapter_id: &str, kind: &str) -> Result<(), StoreError> {
    if tenant_id.is_nil()
        || adapter_id.trim().is_empty()
        || adapter_id.len() > 128
        || kind.trim().is_empty()
        || kind.len() > 128
        || adapter_id.chars().any(char::is_control)
        || kind.chars().any(char::is_control)
    {
        return Err(StoreError::Invariant(
            "invalid bridge sync tenant, adapter, or kind scope".to_owned(),
        ));
    }
    Ok(())
}

fn validate_cursor(cursor: &str) -> Result<(), StoreError> {
    if cursor.len() > MAX_CURSOR || cursor.chars().any(char::is_control) {
        return Err(StoreError::Invariant(
            "bridge sync cursor is invalid or too large".to_owned(),
        ));
    }
    Ok(())
}

fn validate_change(
    expected_kind: &str,
    change: &BridgeSyncChange,
) -> Result<(), BridgeSyncConflict> {
    let reason = |conflict_kind: &str, reason: &str| BridgeSyncConflict {
        operation: change.operation,
        kind: change.kind.clone(),
        external_ref: sanitize(&change.external_id, 512),
        conflict_kind: conflict_kind.to_owned(),
        reason: sanitize(reason, MAX_REASON),
    };
    if change.kind != expected_kind {
        return Err(reason(
            "kind_mismatch",
            "change kind differs from sync request",
        ));
    }
    if change.external_id.trim().is_empty()
        || change.external_id.len() > MAX_EXTERNAL_ID
        || change.external_id.chars().any(char::is_control)
    {
        return Err(reason(
            "invalid_external_id",
            "external record ID is invalid",
        ));
    }
    if !change.body.is_object() {
        return Err(reason(
            "invalid_record_shape",
            "bridge record data must be an object",
        ));
    }
    if change.operation == BridgeSyncOperation::Upsert {
        if let Err(error) = KindRegistry::default().validate(&change.kind, &change.body) {
            return Err(reason("cdm_schema_violation", &error.to_string()));
        }
    } else if !builtin_kind_names().iter().any(|name| *name == change.kind) {
        return Err(reason(
            "unknown_kind",
            "bridge kind is not a built-in CDM kind",
        ));
    }
    Ok(())
}

fn origin_ref(kind: &str, external_id: &str) -> Result<String, BridgeSyncConflict> {
    let value = format!("{kind}:{external_id}");
    if value.len() > 512 {
        return Err(BridgeSyncConflict {
            operation: BridgeSyncOperation::Upsert,
            kind: kind.to_owned(),
            external_ref: sanitize(external_id, 512),
            conflict_kind: "origin_ref_too_large".to_owned(),
            reason: "bridge origin reference exceeds the CDM bound".to_owned(),
        });
    }
    Ok(value)
}

fn validate_conflict(conflict: &BridgeSyncConflict) -> Result<(), StoreError> {
    if conflict.kind.trim().is_empty()
        || conflict.external_ref.trim().is_empty()
        || conflict.conflict_kind.trim().is_empty()
    {
        return Err(StoreError::Invariant(
            "bridge sync conflict metadata cannot be empty".to_owned(),
        ));
    }
    Ok(())
}

fn next_version(version: i64) -> Result<i64, StoreError> {
    version
        .checked_add(1)
        .filter(|value| *value > 0)
        .ok_or_else(|| StoreError::Invariant("bridge entity version overflow".to_owned()))
}

#[allow(clippy::too_many_arguments)]
async fn append_entity_event(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    entity_id: Uuid,
    kind: &str,
    origin: &str,
    origin_ref: &str,
    version: i64,
    event_type: HydraEventType,
    provenance: &EventProvenance,
) -> Result<(), StoreError> {
    let mut event = HydraEventEnvelope::new(
        Uuid::new_v4(),
        event_type,
        canonical_now()?,
        tenant_id,
        provenance.actor.clone(),
        EventDataClass::Private,
        HydraEventPayload::EntityChange {
            operation: match event_type {
                HydraEventType::EntityCreated => "created",
                HydraEventType::EntityUpdated => "updated",
                HydraEventType::EntityDeleted => "deleted",
                _ => "updated",
            }
            .to_owned(),
            version: u64::try_from(version)
                .map_err(|_| StoreError::Invariant("negative entity version".to_owned()))?,
        },
    );
    event.entity = Some(EventEntityRef {
        entity_id,
        kind: kind.to_owned(),
        origin: origin.to_owned(),
        origin_ref: Some(origin_ref.to_owned()),
    });
    provenance.apply(&mut event);
    EventsRepo::append_canonical_with_trace(tx, &event, provenance.trace_context.as_ref()).await
}

fn sanitize(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(limit)
        .collect()
}

fn map_active_run_error(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(database) = &error {
        if database.constraint() == Some("bridge_sync_run_active_unique") {
            return StoreError::Invariant("bridge sync run is already active".to_owned());
        }
    }
    StoreError::Database(error)
}
