use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::StoreError;

pub const MIN_INTERVAL_SECONDS: i64 = 60;
pub const MAX_INTERVAL_SECONDS: i64 = 86_400;
pub const MIN_PAGE_LIMIT: i32 = 1;
pub const MAX_PAGE_LIMIT: i32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeScheduleState {
    Enabled,
    Disabled,
}

impl BridgeScheduleState {
    pub const fn as_bool(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSyncSchedule {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub adapter_id: String,
    pub kind: String,
    pub interval_seconds: i64,
    pub page_limit: i32,
    pub enabled: bool,
    pub next_due_at: OffsetDateTime,
    pub lease_token: Option<Uuid>,
    pub lease_expires_at: Option<OffsetDateTime>,
    pub last_envelope_id: Option<Uuid>,
    pub last_started_at: Option<OffsetDateTime>,
    pub last_finished_at: Option<OffsetDateTime>,
    pub last_error: Option<String>,
    pub revision: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBridgeSyncSchedule {
    pub tenant_id: Uuid,
    pub adapter_id: String,
    pub kind: String,
    pub interval_seconds: i64,
    pub page_limit: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSyncScheduleLease {
    pub schedule: BridgeSyncSchedule,
    pub lease_token: Uuid,
    pub slot_due_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSyncScheduleCompletion {
    pub envelope_id: Option<Uuid>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct BridgeSchedulesRepo {
    pool: PgPool,
}

impl BridgeSchedulesRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        request: NewBridgeSyncSchedule,
    ) -> Result<BridgeSyncSchedule, StoreError> {
        validate_scope(request.tenant_id, &request.adapter_id, &request.kind)?;
        validate_interval(request.interval_seconds)?;
        validate_page_limit(request.page_limit)?;
        let row = sqlx::query_as!(
            BridgeSyncScheduleRow,
            r#"
            INSERT INTO bridge_sync_schedule (
                tenant_id, adapter_id, kind, interval_seconds, page_limit
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id, tenant_id, adapter_id, kind, interval_seconds, page_limit,
                enabled, next_due_at, lease_token, lease_expires_at,
                last_envelope_id, last_started_at, last_finished_at, last_error,
                revision, created_at, updated_at
            "#,
            request.tenant_id,
            request.adapter_id,
            request.kind,
            request.interval_seconds,
            request.page_limit,
        )
        .fetch_one(&self.pool)
        .await?;
        row_to_schedule(row)
    }

    pub async fn list_for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<BridgeSyncSchedule>, StoreError> {
        if tenant_id.is_nil() {
            return Err(StoreError::Invariant(
                "bridge schedule tenant_id cannot be nil".to_owned(),
            ));
        }
        let rows = sqlx::query_as!(
            BridgeSyncScheduleRow,
            r#"
            SELECT
                id, tenant_id, adapter_id, kind, interval_seconds, page_limit,
                enabled, next_due_at, lease_token, lease_expires_at,
                last_envelope_id, last_started_at, last_finished_at, last_error,
                revision, created_at, updated_at
            FROM bridge_sync_schedule
            WHERE tenant_id = $1
            ORDER BY adapter_id, kind, id
            "#,
            tenant_id,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_schedule).collect()
    }

    pub async fn set_state(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
        state: BridgeScheduleState,
    ) -> Result<BridgeSyncSchedule, StoreError> {
        validate_ids(tenant_id, schedule_id)?;
        let row = sqlx::query_as!(
            BridgeSyncScheduleRow,
            r#"
            UPDATE bridge_sync_schedule
            SET enabled = $3,
                next_due_at = CASE WHEN $3 THEN LEAST(next_due_at, now()) ELSE next_due_at END,
                lease_token = CASE WHEN $3 THEN lease_token ELSE NULL END,
                lease_expires_at = CASE WHEN $3 THEN lease_expires_at ELSE NULL END,
                revision = revision + 1,
                updated_at = now()
            WHERE tenant_id = $1 AND id = $2
            RETURNING
                id, tenant_id, adapter_id, kind, interval_seconds, page_limit,
                enabled, next_due_at, lease_token, lease_expires_at,
                last_envelope_id, last_started_at, last_finished_at, last_error,
                revision, created_at, updated_at
            "#,
            tenant_id,
            schedule_id,
            state.as_bool(),
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        row_to_schedule(row)
    }

    pub async fn claim_due(
        &self,
        now: OffsetDateTime,
        lease_for_seconds: i64,
        batch_size: i64,
    ) -> Result<Vec<BridgeSyncScheduleLease>, StoreError> {
        if lease_for_seconds <= 0 || lease_for_seconds > MAX_INTERVAL_SECONDS {
            return Err(StoreError::Invariant(
                "bridge schedule lease duration is out of bounds".to_owned(),
            ));
        }
        if !(1..=100).contains(&batch_size) {
            return Err(StoreError::Invariant(
                "bridge schedule batch size must be 1-100".to_owned(),
            ));
        }
        let lease_expires_at = now + time::Duration::seconds(lease_for_seconds);
        let mut transaction = self.pool.begin().await?;
        let mut leases = Vec::new();
        for _ in 0..batch_size {
            let Some(row) = sqlx::query_as!(
                BridgeSyncScheduleRow,
                r#"
                SELECT
                    s.id, s.tenant_id, s.adapter_id, s.kind, s.interval_seconds,
                    s.page_limit, s.enabled, s.next_due_at, s.lease_token,
                    s.lease_expires_at, s.last_envelope_id, s.last_started_at,
                    s.last_finished_at, s.last_error, s.revision, s.created_at,
                    s.updated_at
                FROM bridge_sync_schedule s
                JOIN bridge_adapter a
                  ON a.tenant_id = s.tenant_id AND a.adapter_id = s.adapter_id
                WHERE s.enabled
                  AND s.next_due_at <= $1
                  AND (s.lease_expires_at IS NULL OR s.lease_expires_at <= $1)
                  AND a.state = 'active'
                ORDER BY s.next_due_at, s.id
                FOR UPDATE OF s SKIP LOCKED
                LIMIT $2
                "#,
                now,
                batch_size,
            )
            .fetch_optional(&mut *transaction)
            .await?
            else {
                break;
            };

            let lease_token = Uuid::new_v4();
            let updated = sqlx::query!(
                r#"
                UPDATE bridge_sync_schedule
                SET lease_token = $3,
                    lease_expires_at = $4,
                    last_started_at = $1,
                    last_error = NULL,
                    revision = revision + 1,
                    updated_at = $1
                WHERE id = $2
                "#,
                now,
                row.id,
                lease_token,
                lease_expires_at,
            )
            .execute(&mut *transaction)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::Invariant(
                    "bridge schedule lease update affected unexpected rows".to_owned(),
                ));
            }
            let schedule = row_to_schedule(BridgeSyncScheduleRow {
                lease_token: Some(lease_token),
                lease_expires_at: Some(lease_expires_at),
                last_started_at: Some(now),
                last_error: None,
                revision: row.revision + 1,
                updated_at: now,
                ..row
            })?;
            leases.push(BridgeSyncScheduleLease {
                slot_due_at: schedule.next_due_at,
                schedule,
                lease_token,
            });
        }
        transaction.commit().await?;
        Ok(leases)
    }

    pub async fn complete_claim(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
        lease_token: Uuid,
        now: OffsetDateTime,
        completion: BridgeSyncScheduleCompletion,
    ) -> Result<BridgeSyncSchedule, StoreError> {
        validate_ids(tenant_id, schedule_id)?;
        if lease_token.is_nil() {
            return Err(StoreError::Invariant(
                "bridge schedule lease token cannot be nil".to_owned(),
            ));
        }
        let error: Option<String> = completion
            .error
            .map(|value| value.chars().take(1024).collect());
        let row = sqlx::query_as!(
            BridgeSyncScheduleRow,
            r#"
            UPDATE bridge_sync_schedule
            SET lease_token = NULL,
                lease_expires_at = NULL,
                last_envelope_id = COALESCE($4::uuid, last_envelope_id),
                last_finished_at = $5::timestamptz,
                last_error = $6::text,
                next_due_at = $5::timestamptz + (interval_seconds * interval '1 second'),
                revision = revision + 1,
                updated_at = $5::timestamptz
            WHERE tenant_id = $1 AND id = $2 AND lease_token = $3
            RETURNING
                id, tenant_id, adapter_id, kind, interval_seconds, page_limit,
                enabled, next_due_at, lease_token, lease_expires_at,
                last_envelope_id, last_started_at, last_finished_at, last_error,
                revision, created_at, updated_at
            "#,
            tenant_id,
            schedule_id,
            lease_token,
            completion.envelope_id,
            now,
            error,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::Conflict(0))?;
        row_to_schedule(row)
    }
}

#[derive(Debug)]
struct BridgeSyncScheduleRow {
    id: Uuid,
    tenant_id: Uuid,
    adapter_id: String,
    kind: String,
    interval_seconds: i64,
    page_limit: i32,
    enabled: bool,
    next_due_at: OffsetDateTime,
    lease_token: Option<Uuid>,
    lease_expires_at: Option<OffsetDateTime>,
    last_envelope_id: Option<Uuid>,
    last_started_at: Option<OffsetDateTime>,
    last_finished_at: Option<OffsetDateTime>,
    last_error: Option<String>,
    revision: i64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

fn row_to_schedule(row: BridgeSyncScheduleRow) -> Result<BridgeSyncSchedule, StoreError> {
    validate_interval(row.interval_seconds)?;
    validate_page_limit(row.page_limit)?;
    Ok(BridgeSyncSchedule {
        id: row.id,
        tenant_id: row.tenant_id,
        adapter_id: row.adapter_id,
        kind: row.kind,
        interval_seconds: row.interval_seconds,
        page_limit: row.page_limit,
        enabled: row.enabled,
        next_due_at: row.next_due_at,
        lease_token: row.lease_token,
        lease_expires_at: row.lease_expires_at,
        last_envelope_id: row.last_envelope_id,
        last_started_at: row.last_started_at,
        last_finished_at: row.last_finished_at,
        last_error: row.last_error,
        revision: row.revision,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn validate_scope(tenant_id: Uuid, adapter_id: &str, kind: &str) -> Result<(), StoreError> {
    if tenant_id.is_nil() || adapter_id.trim().is_empty() || kind.trim().is_empty() {
        return Err(StoreError::Invariant(
            "bridge schedule scope fields cannot be empty".to_owned(),
        ));
    }
    if adapter_id.len() > 128 || kind.len() > 128 {
        return Err(StoreError::Invariant(
            "bridge schedule scope fields are too long".to_owned(),
        ));
    }
    Ok(())
}

fn validate_ids(tenant_id: Uuid, schedule_id: Uuid) -> Result<(), StoreError> {
    if tenant_id.is_nil() || schedule_id.is_nil() {
        return Err(StoreError::Invariant(
            "bridge schedule authority fields cannot be nil".to_owned(),
        ));
    }
    Ok(())
}

fn validate_interval(interval_seconds: i64) -> Result<(), StoreError> {
    if !(MIN_INTERVAL_SECONDS..=MAX_INTERVAL_SECONDS).contains(&interval_seconds) {
        return Err(StoreError::Invariant(
            "bridge schedule interval must be 60-86400 seconds".to_owned(),
        ));
    }
    Ok(())
}

fn validate_page_limit(page_limit: i32) -> Result<(), StoreError> {
    if !(MIN_PAGE_LIMIT..=MAX_PAGE_LIMIT).contains(&page_limit) {
        return Err(StoreError::Invariant(
            "bridge schedule page limit must be 1-100".to_owned(),
        ));
    }
    Ok(())
}
