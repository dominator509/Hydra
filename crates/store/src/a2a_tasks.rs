use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::StoreError;

const MAX_TEXT_BYTES: usize = 512;
const MAX_INPUT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct A2aTask {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub context_id: String,
    pub message_id: String,
    pub workflow: String,
    pub request_hash: String,
    pub requester_principal_id: String,
    pub requester_principal_type: String,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub objective_id: Option<String>,
    pub input: Value,
    pub status: String,
    pub artifact: Option<Value>,
    pub history: Value,
    pub revision: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewA2aTask {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub context_id: String,
    pub message_id: String,
    pub workflow: String,
    pub request_hash: String,
    pub requester_principal_id: String,
    pub requester_principal_type: String,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub objective_id: Option<String>,
    pub input: Value,
    pub history: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct A2aTaskTransition {
    pub tenant_id: Uuid,
    pub id: Uuid,
    pub expected_revision: i64,
    pub expected_status: String,
    pub new_status: String,
    pub artifact: Option<Value>,
    pub history_event: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum A2aTaskResolution {
    Created(A2aTask),
    Existing(A2aTask),
}

impl A2aTaskResolution {
    pub fn task(&self) -> &A2aTask {
        match self {
            Self::Created(task) | Self::Existing(task) => task,
        }
    }
}

#[derive(Clone)]
pub struct A2aTasksRepo {
    pool: PgPool,
}

impl A2aTasksRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_or_get(
        &self,
        request: NewA2aTask,
    ) -> Result<A2aTaskResolution, StoreError> {
        validate_new(&request)?;
        let inserted = sqlx::query_as!(
            A2aTask,
            r#"
            INSERT INTO a2a_task (
                id, tenant_id, context_id, message_id, workflow, request_hash,
                requester_principal_id, requester_principal_type, correlation_id,
                causation_id, objective_id, input, history
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (tenant_id, message_id) DO NOTHING
            RETURNING
                id, tenant_id, context_id, message_id, workflow, request_hash,
                requester_principal_id, requester_principal_type, correlation_id,
                causation_id, objective_id, input, status, artifact, history,
                revision, created_at, updated_at
            "#,
            request.id,
            request.tenant_id,
            request.context_id,
            request.message_id,
            request.workflow,
            request.request_hash,
            request.requester_principal_id,
            request.requester_principal_type,
            request.correlation_id,
            request.causation_id,
            request.objective_id,
            request.input,
            request.history,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(task) = inserted {
            return Ok(A2aTaskResolution::Created(task));
        }

        let existing = self
            .get_by_message(request.tenant_id, &request.message_id)
            .await?;
        if existing.request_hash != request.request_hash {
            return Err(StoreError::IdempotencyConflict);
        }
        Ok(A2aTaskResolution::Existing(existing))
    }

    pub async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<A2aTask, StoreError> {
        sqlx::query_as!(
            A2aTask,
            r#"
            SELECT
                id, tenant_id, context_id, message_id, workflow, request_hash,
                requester_principal_id, requester_principal_type, correlation_id,
                causation_id, objective_id, input, status, artifact, history,
                revision, created_at, updated_at
            FROM a2a_task
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant_id,
            id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)
    }

    pub async fn get_by_message(
        &self,
        tenant_id: Uuid,
        message_id: &str,
    ) -> Result<A2aTask, StoreError> {
        sqlx::query_as!(
            A2aTask,
            r#"
            SELECT
                id, tenant_id, context_id, message_id, workflow, request_hash,
                requester_principal_id, requester_principal_type, correlation_id,
                causation_id, objective_id, input, status, artifact, history,
                revision, created_at, updated_at
            FROM a2a_task
            WHERE tenant_id = $1 AND message_id = $2
            "#,
            tenant_id,
            message_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)
    }

    pub async fn list(
        &self,
        tenant_id: Uuid,
        context_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<A2aTask>, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invariant(
                "A2A task list limit must be between 1 and 100".to_owned(),
            ));
        }
        Ok(sqlx::query_as!(
            A2aTask,
            r#"
            SELECT
                id, tenant_id, context_id, message_id, workflow, request_hash,
                requester_principal_id, requester_principal_type, correlation_id,
                causation_id, objective_id, input, status, artifact, history,
                revision, created_at, updated_at
            FROM a2a_task
            WHERE tenant_id = $1
              AND ($2::text IS NULL OR context_id = $2)
            ORDER BY updated_at DESC, id
            LIMIT $3
            "#,
            tenant_id,
            context_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn transition(&self, request: A2aTaskTransition) -> Result<A2aTask, StoreError> {
        if request.expected_revision < 0
            || !valid_status(&request.new_status)
            || !valid_status(&request.expected_status)
        {
            return Err(StoreError::Invariant(
                "invalid A2A task transition authority".to_owned(),
            ));
        }
        if request.history_event.to_string().len() > MAX_INPUT_BYTES {
            return Err(StoreError::Invariant(
                "A2A task history event is too large".to_owned(),
            ));
        }
        let task = sqlx::query_as!(
            A2aTask,
            r#"
            UPDATE a2a_task
            SET status = $5,
                artifact = COALESCE($6, artifact),
                history = history || jsonb_build_array($7::jsonb),
                revision = revision + 1,
                updated_at = now()
            WHERE tenant_id = $1
              AND id = $2
              AND revision = $3
              AND status = $4
            RETURNING
                id, tenant_id, context_id, message_id, workflow, request_hash,
                requester_principal_id, requester_principal_type, correlation_id,
                causation_id, objective_id, input, status, artifact, history,
                revision, created_at, updated_at
            "#,
            request.tenant_id,
            request.id,
            request.expected_revision,
            request.expected_status,
            request.new_status,
            request.artifact,
            request.history_event,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(task) = task {
            return Ok(task);
        }
        match self.get(request.tenant_id, request.id).await {
            Ok(_) => Err(StoreError::Conflict(request.expected_revision as u64)),
            Err(StoreError::NotFound) => Err(StoreError::NotFound),
            Err(error) => Err(error),
        }
    }
}

fn validate_new(request: &NewA2aTask) -> Result<(), StoreError> {
    if request.id.is_nil() || request.tenant_id.is_nil() {
        return Err(StoreError::Invariant(
            "A2A task identifiers cannot be nil".to_owned(),
        ));
    }
    for (name, value) in [
        ("context_id", request.context_id.as_str()),
        ("message_id", request.message_id.as_str()),
        ("workflow", request.workflow.as_str()),
        (
            "requester_principal_id",
            request.requester_principal_id.as_str(),
        ),
        (
            "requester_principal_type",
            request.requester_principal_type.as_str(),
        ),
        ("correlation_id", request.correlation_id.as_str()),
    ] {
        if value.trim().is_empty() || value.len() > MAX_TEXT_BYTES {
            return Err(StoreError::Invariant(format!(
                "A2A task {name} is empty or too large"
            )));
        }
    }
    if request
        .causation_id
        .as_deref()
        .is_some_and(|value| value.is_empty() || value.len() > MAX_TEXT_BYTES)
        || request
            .objective_id
            .as_deref()
            .is_some_and(|value| value.is_empty() || value.len() > MAX_TEXT_BYTES)
        || request.request_hash.len() != 64
        || !request
            .request_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || request.input.to_string().len() > MAX_INPUT_BYTES
        || !request.history.is_array()
    {
        return Err(StoreError::Invariant(
            "invalid A2A task request metadata".to_owned(),
        ));
    }
    Ok(())
}

fn valid_status(value: &str) -> bool {
    matches!(
        value,
        "submitted" | "working" | "completed" | "canceled" | "failed" | "rejected"
    )
}
