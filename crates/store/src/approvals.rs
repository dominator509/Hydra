use serde_json::Value;
use sqlx::types::Json;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{StoreError, TraceContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approved,
    Rejected,
}

impl ApprovalDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }
}

impl TryFrom<&str> for ApprovalDecision {
    type Error = StoreError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            other => Err(StoreError::Invariant(format!(
                "unknown approval decision '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalAssertion {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub envelope_id: Uuid,
    pub human_actor_id: String,
    pub delegated_by: String,
    pub authentication_strength: String,
    pub approved_at: OffsetDateTime,
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    pub objective_id: Option<String>,
    pub task_id: Option<String>,
    pub decision: ApprovalDecision,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApprovalAssertion {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub envelope_id: Uuid,
    pub human_actor_id: String,
    pub delegated_by: String,
    pub authentication_strength: String,
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    pub objective_id: Option<String>,
    pub task_id: Option<String>,
    pub decision: ApprovalDecision,
    pub comment: Option<String>,
}

struct ApprovalRow {
    id: Uuid,
    tenant_id: Uuid,
    envelope_id: Uuid,
    human_actor_id: String,
    delegated_by: String,
    authentication_strength: String,
    approved_at: OffsetDateTime,
    request_id: Option<String>,
    correlation_id: Option<String>,
    objective_id: Option<String>,
    task_id: Option<String>,
    decision: String,
    comment: Option<String>,
}

struct EnvelopeDocRow {
    doc: Json<Value>,
    revision: i64,
}

#[derive(Clone)]
pub struct ApprovalsRepo {
    pool: PgPool,
}

impl ApprovalsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        assertion: NewApprovalAssertion,
    ) -> Result<ApprovalAssertion, StoreError> {
        validate_assertion(&assertion)?;
        let envelope_doc = sqlx::query_as!(
            EnvelopeDocRow,
            r#"
            SELECT
                doc as "doc!: Json<Value>",
                revision
            FROM envelope
            WHERE tenant_id = $1 AND id = $2
            "#,
            assertion.tenant_id,
            assertion.envelope_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        let envelope: governor::ActionEnvelope = serde_json::from_value(envelope_doc.doc.0)
            .map_err(|error| StoreError::Invariant(format!("deserialize envelope: {error}")))?;
        let _ = envelope_doc.revision;
        if envelope.state != governor::EnvelopeState::PendingApproval {
            return Err(StoreError::ApprovalDenied);
        }
        let proposer = envelope
            .invocation
            .external_actor_id
            .as_deref()
            .or_else(|| {
                envelope
                    .history
                    .first()
                    .map(|transition| transition.actor.as_str())
            });
        if proposer == Some(assertion.human_actor_id.as_str()) {
            return Err(StoreError::ApprovalDenied);
        }

        let row = sqlx::query_as!(
            ApprovalRow,
            r#"
            INSERT INTO approval_assertion (
                id,
                tenant_id,
                envelope_id,
                human_actor_id,
                delegated_by,
                authentication_strength,
                request_id,
                correlation_id,
                objective_id,
                task_id,
                decision,
                comment
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING
                id,
                tenant_id,
                envelope_id,
                human_actor_id,
                delegated_by,
                authentication_strength,
                approved_at,
                request_id,
                correlation_id,
                objective_id,
                task_id,
                decision,
                comment
            "#,
            assertion.id,
            assertion.tenant_id,
            assertion.envelope_id,
            assertion.human_actor_id,
            assertion.delegated_by,
            assertion.authentication_strength,
            assertion.request_id,
            assertion.correlation_id,
            assertion.objective_id,
            assertion.task_id,
            assertion.decision.as_str(),
            assertion.comment,
        )
        .fetch_one(&self.pool)
        .await?;
        row_to_assertion(row)
    }

    pub async fn create_and_transition(
        &self,
        assertion: NewApprovalAssertion,
        actor: &str,
        clock: &dyn governor::Clock,
    ) -> Result<(ApprovalAssertion, governor::ActionEnvelope), StoreError> {
        self.create_and_transition_with_trace(assertion, actor, clock, None)
            .await
    }

    pub async fn create_and_transition_with_trace(
        &self,
        assertion: NewApprovalAssertion,
        actor: &str,
        clock: &dyn governor::Clock,
        trace_context: Option<&TraceContext>,
    ) -> Result<(ApprovalAssertion, governor::ActionEnvelope), StoreError> {
        validate_assertion(&assertion)?;
        if actor != assertion.human_actor_id || actor.trim().is_empty() {
            return Err(StoreError::ApprovalDenied);
        }
        let mut tx = self.pool.begin().await?;
        let envelope_doc = sqlx::query_as!(
            EnvelopeDocRow,
            r#"
            SELECT
                doc as "doc!: Json<Value>",
                revision
            FROM envelope
            WHERE tenant_id = $1 AND id = $2
            FOR UPDATE
            "#,
            assertion.tenant_id,
            assertion.envelope_id,
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let expected_revision = u64::try_from(envelope_doc.revision).map_err(|_| {
            StoreError::Invariant(format!(
                "negative envelope revision for {}",
                assertion.envelope_id
            ))
        })?;
        let mut envelope: governor::ActionEnvelope = serde_json::from_value(envelope_doc.doc.0)
            .map_err(|error| StoreError::Invariant(format!("deserialize envelope: {error}")))?;
        if envelope.tenant != assertion.tenant_id
            || envelope.state != governor::EnvelopeState::PendingApproval
        {
            return Err(StoreError::ApprovalDenied);
        }
        let proposer = proposer(&envelope);
        if proposer == Some(assertion.human_actor_id.as_str()) {
            return Err(StoreError::ApprovalDenied);
        }

        let stored = insert_assertion(&mut tx, &assertion).await?;
        let to = match assertion.decision {
            ApprovalDecision::Approved => {
                envelope.invocation.approval_id = Some(assertion.id.to_string());
                governor::EnvelopeState::Approved
            }
            ApprovalDecision::Rejected => governor::EnvelopeState::Rejected,
        };
        envelope.transition(to, actor, clock)?;
        crate::envelopes::persist_transition(
            &mut tx,
            assertion.tenant_id,
            expected_revision,
            &envelope,
            trace_context,
        )
        .await?;
        crate::envelopes::append_transition_records(&mut tx, &envelope, trace_context).await?;
        tx.commit().await?;
        Ok((stored, envelope))
    }

    pub async fn get(
        &self,
        tenant_id: Uuid,
        approval_id: Uuid,
    ) -> Result<ApprovalAssertion, StoreError> {
        let row = sqlx::query_as!(
            ApprovalRow,
            r#"
            SELECT
                id,
                tenant_id,
                envelope_id,
                human_actor_id,
                delegated_by,
                authentication_strength,
                approved_at,
                request_id,
                correlation_id,
                objective_id,
                task_id,
                decision,
                comment
            FROM approval_assertion
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant_id,
            approval_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        row_to_assertion(row)
    }

    pub async fn latest_approved(
        &self,
        tenant_id: Uuid,
        envelope_id: Uuid,
    ) -> Result<Option<ApprovalAssertion>, StoreError> {
        let row = sqlx::query_as!(
            ApprovalRow,
            r#"
            SELECT
                id,
                tenant_id,
                envelope_id,
                human_actor_id,
                delegated_by,
                authentication_strength,
                approved_at,
                request_id,
                correlation_id,
                objective_id,
                task_id,
                decision,
                comment
            FROM approval_assertion
            WHERE tenant_id = $1
              AND envelope_id = $2
              AND decision = 'approved'
            ORDER BY approved_at DESC, id DESC
            LIMIT 1
            "#,
            tenant_id,
            envelope_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_assertion).transpose()
    }
}

async fn insert_assertion(
    tx: &mut Transaction<'_, Postgres>,
    assertion: &NewApprovalAssertion,
) -> Result<ApprovalAssertion, StoreError> {
    let row = sqlx::query_as!(
        ApprovalRow,
        r#"
        INSERT INTO approval_assertion (
            id,
            tenant_id,
            envelope_id,
            human_actor_id,
            delegated_by,
            authentication_strength,
            request_id,
            correlation_id,
            objective_id,
            task_id,
            decision,
            comment
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING
            id,
            tenant_id,
            envelope_id,
            human_actor_id,
            delegated_by,
            authentication_strength,
            approved_at,
            request_id,
            correlation_id,
            objective_id,
            task_id,
            decision,
            comment
        "#,
        assertion.id,
        assertion.tenant_id,
        assertion.envelope_id,
        assertion.human_actor_id,
        assertion.delegated_by,
        assertion.authentication_strength,
        assertion.request_id,
        assertion.correlation_id,
        assertion.objective_id,
        assertion.task_id,
        assertion.decision.as_str(),
        assertion.comment,
    )
    .fetch_one(&mut **tx)
    .await?;
    row_to_assertion(row)
}

fn proposer(envelope: &governor::ActionEnvelope) -> Option<&str> {
    envelope
        .invocation
        .external_actor_id
        .as_deref()
        .or_else(|| {
            envelope
                .history
                .first()
                .map(|transition| transition.actor.as_str())
        })
}

fn validate_assertion(assertion: &NewApprovalAssertion) -> Result<(), StoreError> {
    let optional_ids_valid = [
        assertion.request_id.as_deref(),
        assertion.correlation_id.as_deref(),
        assertion.objective_id.as_deref(),
        assertion.task_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    .all(|value| !value.trim().is_empty() && value.len() <= 200);
    if assertion.id.is_nil()
        || assertion.tenant_id.is_nil()
        || assertion.envelope_id.is_nil()
        || assertion.human_actor_id.trim().is_empty()
        || assertion.human_actor_id.len() > 200
        || assertion.delegated_by.trim().is_empty()
        || assertion.delegated_by.len() > 200
        || assertion.authentication_strength.trim().is_empty()
        || assertion.authentication_strength.len() > 200
        || !optional_ids_valid
        || assertion
            .comment
            .as_ref()
            .is_some_and(|comment| comment.len() > 2000)
    {
        return Err(StoreError::ApprovalDenied);
    }
    Ok(())
}

fn row_to_assertion(row: ApprovalRow) -> Result<ApprovalAssertion, StoreError> {
    Ok(ApprovalAssertion {
        id: row.id,
        tenant_id: row.tenant_id,
        envelope_id: row.envelope_id,
        human_actor_id: row.human_actor_id,
        delegated_by: row.delegated_by,
        authentication_strength: row.authentication_strength,
        approved_at: row.approved_at,
        request_id: row.request_id,
        correlation_id: row.correlation_id,
        objective_id: row.objective_id,
        task_id: row.task_id,
        decision: ApprovalDecision::try_from(row.decision.as_str())?,
        comment: row.comment,
    })
}
