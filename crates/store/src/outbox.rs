use std::time::Duration;

use cdm::HydraEventEnvelope;
use serde_json::Value;
use sqlx::types::Json;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{StoreError, TraceContext};

#[derive(Debug, Clone, PartialEq)]
pub struct OutboxRecord {
    pub id: i64,
    pub event_id: Uuid,
    pub subject: String,
    pub event: HydraEventEnvelope,
    pub created_at: OffsetDateTime,
    pub published_at: Option<OffsetDateTime>,
    pub attempt_count: i32,
    pub last_error: Option<String>,
    pub parked_at: Option<OffsetDateTime>,
    pub jetstream_sequence: Option<i64>,
    pub claim_token: Option<Uuid>,
    pub claimed_at: Option<OffsetDateTime>,
    pub trace_context: Option<TraceContext>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutboxClaimBatch {
    pub records: Vec<OutboxRecord>,
    pub parked_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxFailureDisposition {
    Retry,
    Park,
}

struct OutboxRow {
    id: i64,
    event_id: Uuid,
    subject: String,
    event: Json<Value>,
    created_at: OffsetDateTime,
    published_at: Option<OffsetDateTime>,
    attempt_count: i32,
    last_error: Option<String>,
    parked_at: Option<OffsetDateTime>,
    jetstream_sequence: Option<i64>,
    claim_token: Option<Uuid>,
    claimed_at: Option<OffsetDateTime>,
    trace_context: Option<Json<Value>>,
}

#[derive(Clone)]
pub struct OutboxRepo {
    pool: PgPool,
}

impl OutboxRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_by_event_id(&self, event_id: Uuid) -> Result<OutboxRecord, StoreError> {
        let row = sqlx::query_as!(
            OutboxRow,
            r#"
            SELECT
                id,
                event_id,
                subject,
                event as "event!: Json<Value>",
                created_at,
                published_at,
                attempt_count,
                last_error,
                parked_at,
                jetstream_sequence,
                claim_token,
                claimed_at,
                trace_context as "trace_context: Json<Value>"
            FROM outbox
            WHERE event_id = $1
            "#,
            event_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        row_to_record(row)
    }

    pub async fn claim_pending(
        &self,
        limit: i64,
        lease_timeout: Duration,
    ) -> Result<OutboxClaimBatch, StoreError> {
        let lease_seconds = i64::try_from(lease_timeout.as_secs()).map_err(|_| {
            StoreError::Invariant("outbox lease timeout exceeds database range".to_owned())
        })?;
        if limit <= 0 || lease_seconds <= 0 {
            return Err(StoreError::Invariant(
                "outbox claim limit and lease timeout must be positive".to_owned(),
            ));
        }

        let claim_token = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query_as!(
            OutboxRow,
            r#"
            WITH candidates AS (
                SELECT id
                FROM outbox
                WHERE published_at IS NULL
                  AND parked_at IS NULL
                  AND (
                      claimed_at IS NULL
                      OR claimed_at < now() - ($2::bigint * interval '1 second')
                  )
                ORDER BY id
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE outbox AS target
            SET claim_token = $3,
                claimed_at = now(),
                attempt_count = target.attempt_count + 1,
                last_error = NULL
            FROM candidates
            WHERE target.id = candidates.id
            RETURNING
                target.id,
                target.event_id,
                target.subject,
                target.event as "event!: Json<Value>",
                target.created_at,
                target.published_at,
                target.attempt_count,
                target.last_error,
                target.parked_at,
                target.jetstream_sequence,
                target.claim_token,
                target.claimed_at,
                target.trace_context as "trace_context: Json<Value>"
            "#,
            limit,
            lease_seconds,
            claim_token,
        )
        .fetch_all(&mut *tx)
        .await?;

        let mut records = Vec::with_capacity(rows.len());
        let mut parked_count = 0;
        for row in rows {
            let row_id = row.id;
            match row_to_record(row) {
                Ok(record) => records.push(record),
                Err(_) => {
                    let result = sqlx::query!(
                        r#"
                        UPDATE outbox
                        SET parked_at = now(),
                            last_error = 'canonical_event_invalid',
                            claim_token = NULL,
                            claimed_at = NULL
                        WHERE id = $1 AND claim_token = $2
                        "#,
                        row_id,
                        claim_token,
                    )
                    .execute(&mut *tx)
                    .await?;
                    if result.rows_affected() != 1 {
                        return Err(StoreError::OutboxClaimLost);
                    }
                    parked_count += 1;
                }
            }
        }

        tx.commit().await?;
        Ok(OutboxClaimBatch {
            records,
            parked_count,
        })
    }

    pub async fn mark_published(
        &self,
        id: i64,
        claim_token: Uuid,
        jetstream_sequence: u64,
    ) -> Result<(), StoreError> {
        let jetstream_sequence = i64::try_from(jetstream_sequence).map_err(|_| {
            StoreError::Invariant("JetStream sequence exceeds database range".to_owned())
        })?;
        if jetstream_sequence <= 0 {
            return Err(StoreError::Invariant(
                "JetStream acknowledgement sequence must be positive".to_owned(),
            ));
        }

        let result = sqlx::query!(
            r#"
            UPDATE outbox
            SET published_at = now(),
                jetstream_sequence = $3,
                last_error = NULL,
                claim_token = NULL,
                claimed_at = NULL
            WHERE id = $1
              AND claim_token = $2
              AND published_at IS NULL
              AND parked_at IS NULL
            "#,
            id,
            claim_token,
            jetstream_sequence,
        )
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::OutboxClaimLost);
        }
        Ok(())
    }

    pub async fn record_failure(
        &self,
        id: i64,
        claim_token: Uuid,
        reason_code: &str,
        disposition: OutboxFailureDisposition,
    ) -> Result<(), StoreError> {
        if reason_code.trim().is_empty()
            || reason_code.len() > 128
            || !reason_code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        {
            return Err(StoreError::Invariant(
                "outbox failure reason must be a redacted snake_case code".to_owned(),
            ));
        }
        let park = disposition == OutboxFailureDisposition::Park;
        let result = sqlx::query!(
            r#"
            UPDATE outbox
            SET last_error = $3,
                parked_at = CASE WHEN $4 THEN now() ELSE NULL END,
                claim_token = NULL,
                claimed_at = NULL
            WHERE id = $1
              AND claim_token = $2
              AND published_at IS NULL
            "#,
            id,
            claim_token,
            reason_code,
            park,
        )
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::OutboxClaimLost);
        }
        Ok(())
    }

    pub(crate) async fn append(
        tx: &mut Transaction<'_, Postgres>,
        event: &HydraEventEnvelope,
        document: &Value,
        trace_context: Option<&TraceContext>,
    ) -> Result<(), StoreError> {
        let trace_context = trace_context
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| StoreError::Invariant(format!("serialize trace context: {error}")))?;
        sqlx::query!(
            r#"
            INSERT INTO outbox (event_id, subject, event, trace_context)
            VALUES ($1, $2, $3, $4)
            "#,
            event.event_id,
            event.subject,
            document.clone(),
            trace_context,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

fn row_to_record(row: OutboxRow) -> Result<OutboxRecord, StoreError> {
    let event = serde_json::from_value::<HydraEventEnvelope>(row.event.0)
        .map_err(|error| StoreError::Invariant(format!("deserialize outbox event: {error}")))?;
    event.validate()?;
    if event.event_id != row.event_id || event.subject != row.subject {
        return Err(StoreError::Invariant(
            "outbox columns do not match canonical event document".to_owned(),
        ));
    }
    let trace_context = row
        .trace_context
        .map(|value| serde_json::from_value::<TraceContext>(value.0))
        .transpose()
        .map_err(|error| StoreError::Invariant(format!("deserialize trace context: {error}")))?;
    if let Some(trace_context) = &trace_context {
        trace_context.validate()?;
    }
    Ok(OutboxRecord {
        id: row.id,
        event_id: row.event_id,
        subject: row.subject,
        event,
        created_at: row.created_at,
        published_at: row.published_at,
        attempt_count: row.attempt_count,
        last_error: row.last_error,
        parked_at: row.parked_at,
        jetstream_sequence: row.jetstream_sequence,
        claim_token: row.claim_token,
        claimed_at: row.claimed_at,
        trace_context,
    })
}
