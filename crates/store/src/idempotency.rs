use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{StoreError, TraceContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub origin_system: String,
    pub idempotency_key: String,
    pub capability: String,
    pub request_hash: String,
    pub envelope_id: Uuid,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewIdempotencyRecord {
    pub tenant_id: Uuid,
    pub origin_system: String,
    pub idempotency_key: String,
    pub capability: String,
    pub request_hash: String,
    pub envelope_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyResolution {
    Recorded(IdempotencyRecord),
    Existing(IdempotencyRecord),
}

impl IdempotencyResolution {
    pub fn record(&self) -> &IdempotencyRecord {
        match self {
            Self::Recorded(record) | Self::Existing(record) => record,
        }
    }
}

#[derive(Clone)]
pub struct IdempotencyRepo {
    pool: PgPool,
}

impl IdempotencyRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn record(
        &self,
        request: NewIdempotencyRecord,
    ) -> Result<IdempotencyResolution, StoreError> {
        validate_request(&request)?;

        let inserted = sqlx::query_as!(
            IdempotencyRecord,
            r#"
            INSERT INTO idempotency_record (
                tenant_id,
                origin_system,
                idempotency_key,
                capability,
                request_hash,
                envelope_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, origin_system, idempotency_key, capability)
            DO NOTHING
            RETURNING
                id,
                tenant_id,
                origin_system,
                idempotency_key,
                capability,
                request_hash,
                envelope_id,
                created_at
            "#,
            request.tenant_id,
            request.origin_system,
            request.idempotency_key,
            request.capability,
            request.request_hash,
            request.envelope_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(record) = inserted {
            return Ok(IdempotencyResolution::Recorded(record));
        }

        let existing = self
            .get(
                request.tenant_id,
                &request.origin_system,
                &request.idempotency_key,
                &request.capability,
            )
            .await?;
        if existing.request_hash != request.request_hash {
            return Err(StoreError::IdempotencyConflict);
        }
        Ok(IdempotencyResolution::Existing(existing))
    }

    pub async fn resolve_or_create_envelope(
        &self,
        request: NewIdempotencyRecord,
        envelope: &governor::ActionEnvelope,
    ) -> Result<IdempotencyResolution, StoreError> {
        self.resolve_or_create_envelope_with_trace(request, envelope, None)
            .await
    }

    pub async fn resolve_or_create_envelope_with_trace(
        &self,
        request: NewIdempotencyRecord,
        envelope: &governor::ActionEnvelope,
        trace_context: Option<&TraceContext>,
    ) -> Result<IdempotencyResolution, StoreError> {
        validate_request(&request)?;
        if envelope.id != request.envelope_id
            || envelope.tenant != request.tenant_id
            || envelope.history.len() > 1
        {
            return Err(StoreError::Invariant(
                "idempotent proposal envelope does not match its authority record".to_owned(),
            ));
        }

        let mut tx = self.pool.begin().await?;
        let trace_document = trace_context
            .map(|trace_context| {
                trace_context.validate()?;
                serde_json::to_value(trace_context).map_err(|error| {
                    StoreError::Invariant(format!("serialize trace context: {error}"))
                })
            })
            .transpose()?;
        let envelope_insert = sqlx::query!(
            r#"
            INSERT INTO envelope (id, tenant_id, state, doc, trace_context)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO NOTHING
            "#,
            envelope.id,
            envelope.tenant,
            crate::envelopes::state_name(envelope.state),
            serde_json::to_value(envelope)
                .map_err(|error| StoreError::Invariant(format!("serialize envelope: {error}")))?,
            trace_document,
        )
        .execute(&mut *tx)
        .await?;
        if envelope_insert.rows_affected() != 1 {
            tx.rollback().await?;
            return Err(StoreError::Invariant(
                "generated proposal envelope identifier already exists".to_owned(),
            ));
        }

        let inserted = sqlx::query_as!(
            IdempotencyRecord,
            r#"
            INSERT INTO idempotency_record (
                tenant_id,
                origin_system,
                idempotency_key,
                capability,
                request_hash,
                envelope_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, origin_system, idempotency_key, capability)
            DO NOTHING
            RETURNING
                id,
                tenant_id,
                origin_system,
                idempotency_key,
                capability,
                request_hash,
                envelope_id,
                created_at
            "#,
            request.tenant_id,
            &request.origin_system,
            &request.idempotency_key,
            &request.capability,
            &request.request_hash,
            request.envelope_id,
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(record) = inserted {
            crate::envelopes::append_proposed_event(
                &mut tx,
                envelope,
                Some(&request.capability),
                trace_context,
            )
            .await?;
            if !envelope.history.is_empty() {
                crate::envelopes::append_transition_records(&mut tx, envelope, trace_context)
                    .await?;
            }
            tx.commit().await?;
            return Ok(IdempotencyResolution::Recorded(record));
        }

        tx.rollback().await?;
        let existing = self
            .get(
                request.tenant_id,
                &request.origin_system,
                &request.idempotency_key,
                &request.capability,
            )
            .await?;
        if existing.request_hash != request.request_hash {
            return Err(StoreError::IdempotencyConflict);
        }
        Ok(IdempotencyResolution::Existing(existing))
    }

    pub async fn get(
        &self,
        tenant_id: Uuid,
        origin_system: &str,
        idempotency_key: &str,
        capability: &str,
    ) -> Result<IdempotencyRecord, StoreError> {
        sqlx::query_as!(
            IdempotencyRecord,
            r#"
            SELECT
                id,
                tenant_id,
                origin_system,
                idempotency_key,
                capability,
                request_hash,
                envelope_id,
                created_at
            FROM idempotency_record
            WHERE tenant_id = $1
              AND origin_system = $2
              AND idempotency_key = $3
              AND capability = $4
            "#,
            tenant_id,
            origin_system,
            idempotency_key,
            capability,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)
    }
}

fn validate_request(request: &NewIdempotencyRecord) -> Result<(), StoreError> {
    if request.tenant_id.is_nil()
        || request.envelope_id.is_nil()
        || request.origin_system.trim().is_empty()
        || request.origin_system.len() > 128
        || request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 200
        || request.capability.trim().is_empty()
        || request.capability.len() > 200
        || request.request_hash.len() != 64
        || !request
            .request_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(StoreError::Invariant(
            "invalid idempotency record authority or hash".to_owned(),
        ));
    }
    Ok(())
}
