use serde_json::Value;
use sqlx::types::Json;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Verified,
    Failed,
}

impl ExecutionOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for ExecutionOutcome {
    type Error = StoreError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "verified" => Ok(Self::Verified),
            "failed" => Ok(Self::Failed),
            other => Err(StoreError::Invariant(format!(
                "unknown execution outcome '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionReceipt {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub envelope_id: Uuid,
    pub capability: String,
    pub handler: String,
    pub outcome: ExecutionOutcome,
    pub affected_targets: Vec<Uuid>,
    pub details: Value,
    pub invocation: governor::InvocationContext,
    pub recorded_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewExecutionReceipt {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub envelope_id: Uuid,
    pub capability: String,
    pub handler: String,
    pub outcome: ExecutionOutcome,
    pub affected_targets: Vec<Uuid>,
    pub details: Value,
    pub invocation: governor::InvocationContext,
}

struct ExecutionReceiptRow {
    id: Uuid,
    tenant_id: Uuid,
    envelope_id: Uuid,
    capability: String,
    handler: String,
    outcome: String,
    affected_targets: Vec<Uuid>,
    details: Json<Value>,
    invocation: Json<Value>,
    recorded_at: OffsetDateTime,
}

#[derive(Clone)]
pub struct ExecutionReceiptsRepo {
    pool: PgPool,
}

impl ExecutionReceiptsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_for_envelope(
        &self,
        tenant: Uuid,
        envelope_id: Uuid,
    ) -> Result<ExecutionReceipt, StoreError> {
        let row = sqlx::query_as!(
            ExecutionReceiptRow,
            r#"
            SELECT
                id,
                tenant_id,
                envelope_id,
                capability,
                handler,
                outcome,
                affected_targets,
                details as "details!: Json<Value>",
                invocation as "invocation!: Json<Value>",
                recorded_at
            FROM execution_receipt
            WHERE tenant_id = $1 AND envelope_id = $2
            "#,
            tenant,
            envelope_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        row_to_receipt(row)
    }
}

pub(crate) async fn insert(
    tx: &mut Transaction<'_, Postgres>,
    receipt: &NewExecutionReceipt,
) -> Result<ExecutionReceipt, StoreError> {
    validate(receipt)?;
    let invocation = serde_json::to_value(&receipt.invocation)
        .map_err(|error| StoreError::Invariant(format!("serialize invocation: {error}")))?;
    let row = sqlx::query_as!(
        ExecutionReceiptRow,
        r#"
        INSERT INTO execution_receipt (
            id,
            tenant_id,
            envelope_id,
            capability,
            handler,
            outcome,
            affected_targets,
            details,
            invocation
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING
            id,
            tenant_id,
            envelope_id,
            capability,
            handler,
            outcome,
            affected_targets,
            details as "details!: Json<Value>",
            invocation as "invocation!: Json<Value>",
            recorded_at
        "#,
        receipt.id,
        receipt.tenant_id,
        receipt.envelope_id,
        receipt.capability,
        receipt.handler,
        receipt.outcome.as_str(),
        &receipt.affected_targets,
        receipt.details.clone(),
        invocation,
    )
    .fetch_one(&mut **tx)
    .await?;

    row_to_receipt(row)
}

fn validate(receipt: &NewExecutionReceipt) -> Result<(), StoreError> {
    if receipt.id.is_nil()
        || receipt.tenant_id.is_nil()
        || receipt.envelope_id.is_nil()
        || receipt.capability.trim().is_empty()
        || receipt.handler.trim().is_empty()
        || receipt.affected_targets.is_empty()
    {
        return Err(StoreError::Invariant(
            "invalid execution receipt".to_owned(),
        ));
    }
    Ok(())
}

fn row_to_receipt(row: ExecutionReceiptRow) -> Result<ExecutionReceipt, StoreError> {
    Ok(ExecutionReceipt {
        id: row.id,
        tenant_id: row.tenant_id,
        envelope_id: row.envelope_id,
        capability: row.capability,
        handler: row.handler,
        outcome: ExecutionOutcome::try_from(row.outcome.as_str())?,
        affected_targets: row.affected_targets,
        details: row.details.0,
        invocation: serde_json::from_value(row.invocation.0)
            .map_err(|error| StoreError::Invariant(format!("deserialize invocation: {error}")))?,
        recorded_at: row.recorded_at,
    })
}
