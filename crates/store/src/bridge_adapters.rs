use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeAdapterState {
    Inactive,
    Activating,
    Active,
    Paused,
    Failed,
}

impl BridgeAdapterState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Activating => "activating",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for BridgeAdapterState {
    type Error = StoreError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "inactive" => Ok(Self::Inactive),
            "activating" => Ok(Self::Activating),
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "failed" => Ok(Self::Failed),
            other => Err(StoreError::Invariant(format!(
                "unknown bridge adapter state '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BridgeAdapterRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub adapter_id: String,
    pub component_ref: String,
    pub component_sha256: String,
    pub grant_config: serde_json::Value,
    pub config: serde_json::Value,
    pub descriptor: Option<serde_json::Value>,
    pub state: BridgeAdapterState,
    pub last_error: Option<String>,
    pub revision: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BridgeAdapterTransitionRecord {
    pub id: i64,
    pub tenant_id: Uuid,
    pub adapter_id: String,
    pub revision: i64,
    pub from_state: Option<BridgeAdapterState>,
    pub to_state: BridgeAdapterState,
    pub event: serde_json::Value,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewBridgeAdapter {
    pub tenant_id: Uuid,
    pub adapter_id: String,
    pub component_ref: String,
    pub component_sha256: String,
    pub grant_config: serde_json::Value,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BridgeAdapterTransition {
    pub tenant_id: Uuid,
    pub adapter_id: String,
    pub expected_revision: i64,
    pub expected_state: BridgeAdapterState,
    pub new_state: BridgeAdapterState,
    pub descriptor: Option<serde_json::Value>,
    pub last_error: Option<String>,
    pub event: serde_json::Value,
}

impl NewBridgeAdapter {
    fn validate(&self) -> Result<(), StoreError> {
        if self.tenant_id.is_nil() {
            return Err(StoreError::Invariant(
                "bridge adapter tenant_id cannot be nil".to_owned(),
            ));
        }
        if self.adapter_id.trim().is_empty() || self.adapter_id.len() > 128 {
            return Err(StoreError::Invariant(
                "bridge adapter_id must be 1-128 characters".to_owned(),
            ));
        }
        if self.adapter_id.chars().any(char::is_control) {
            return Err(StoreError::Invariant(
                "bridge adapter_id cannot contain control characters".to_owned(),
            ));
        }
        if self.component_ref.trim().is_empty() || self.component_ref.len() > 256 {
            return Err(StoreError::Invariant(
                "bridge component_ref must be 1-256 characters".to_owned(),
            ));
        }
        if self.component_sha256.len() != 64
            || !self
                .component_sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
        {
            return Err(StoreError::Invariant(
                "bridge component_sha256 must be lowercase hexadecimal SHA-256".to_owned(),
            ));
        }
        if !self.grant_config.is_object() {
            return Err(StoreError::Invariant(
                "bridge grant_config must be a JSON object".to_owned(),
            ));
        }
        if !self.config.is_object() {
            return Err(StoreError::Invariant(
                "bridge config must be a JSON object".to_owned(),
            ));
        }
        Ok(())
    }
}

impl BridgeAdapterTransition {
    fn validate(&self) -> Result<(), StoreError> {
        if self.tenant_id.is_nil() || self.adapter_id.trim().is_empty() {
            return Err(StoreError::Invariant(
                "bridge transition authority fields cannot be empty".to_owned(),
            ));
        }
        if self.expected_revision < 0 {
            return Err(StoreError::Invariant(
                "bridge transition revision cannot be negative".to_owned(),
            ));
        }
        if self.expected_state == self.new_state {
            return Err(StoreError::Invariant(
                "bridge transition must change state".to_owned(),
            ));
        }
        if !self.event.is_object() {
            return Err(StoreError::Invariant(
                "bridge transition event must be a JSON object".to_owned(),
            ));
        }
        Ok(())
    }
}

struct BridgeAdapterRow {
    id: Uuid,
    tenant_id: Uuid,
    adapter_id: String,
    component_ref: String,
    component_sha256: String,
    grant_config: serde_json::Value,
    config: serde_json::Value,
    descriptor: Option<serde_json::Value>,
    state: String,
    last_error: Option<String>,
    revision: i64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(Clone)]
pub struct BridgeAdaptersRepo {
    pool: PgPool,
}

impl BridgeAdaptersRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        adapter: NewBridgeAdapter,
    ) -> Result<BridgeAdapterRecord, StoreError> {
        adapter.validate()?;
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query_as!(
            BridgeAdapterRow,
            r#"
            INSERT INTO bridge_adapter (
                tenant_id,
                adapter_id,
                component_ref,
                component_sha256,
                grant_config,
                config
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                id,
                tenant_id,
                adapter_id,
                component_ref,
                component_sha256,
                grant_config,
                config,
                descriptor,
                state,
                last_error,
                revision,
                created_at,
                updated_at
            "#,
            adapter.tenant_id,
            adapter.adapter_id,
            adapter.component_ref,
            adapter.component_sha256,
            adapter.grant_config,
            adapter.config,
        )
        .fetch_one(&mut *transaction)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO bridge_adapter_transition (
                tenant_id,
                adapter_id,
                revision,
                from_state,
                to_state,
                event
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            row.tenant_id,
            row.adapter_id,
            row.revision,
            None::<String>,
            row.state,
            serde_json::json!({"event": "registered"}),
        )
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        row_to_record(row)
    }

    pub async fn get(
        &self,
        tenant_id: Uuid,
        adapter_id: &str,
    ) -> Result<Option<BridgeAdapterRecord>, StoreError> {
        let row = sqlx::query_as!(
            BridgeAdapterRow,
            r#"
            SELECT
                id,
                tenant_id,
                adapter_id,
                component_ref,
                component_sha256,
                grant_config,
                config,
                descriptor,
                state,
                last_error,
                revision,
                created_at,
                updated_at
            FROM bridge_adapter
            WHERE tenant_id = $1 AND adapter_id = $2
            "#,
            tenant_id,
            adapter_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_record).transpose()
    }

    pub async fn list_for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<BridgeAdapterRecord>, StoreError> {
        let rows = sqlx::query_as!(
            BridgeAdapterRow,
            r#"
            SELECT
                id,
                tenant_id,
                adapter_id,
                component_ref,
                component_sha256,
                grant_config,
                config,
                descriptor,
                state,
                last_error,
                revision,
                created_at,
                updated_at
            FROM bridge_adapter
            WHERE tenant_id = $1
            ORDER BY adapter_id
            "#,
            tenant_id,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_record).collect()
    }

    pub async fn transition(
        &self,
        request: BridgeAdapterTransition,
    ) -> Result<BridgeAdapterRecord, StoreError> {
        request.validate()?;
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query_as!(
            BridgeAdapterRow,
            r#"
            UPDATE bridge_adapter
            SET state = $4,
                descriptor = COALESCE($5, descriptor),
                last_error = $6,
                revision = revision + 1,
                updated_at = now()
            WHERE tenant_id = $1
              AND adapter_id = $2
              AND revision = $3
              AND state = $7
            RETURNING
                id,
                tenant_id,
                adapter_id,
                component_ref,
                component_sha256,
                grant_config,
                config,
                descriptor,
                state,
                last_error,
                revision,
                created_at,
                updated_at
            "#,
            request.tenant_id,
            request.adapter_id,
            request.expected_revision,
            request.new_state.as_str(),
            request.descriptor,
            request.last_error,
            request.expected_state.as_str(),
        )
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(StoreError::Conflict(request.expected_revision as u64))?;

        sqlx::query!(
            r#"
            INSERT INTO bridge_adapter_transition (
                tenant_id,
                adapter_id,
                revision,
                from_state,
                to_state,
                event
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            row.tenant_id,
            row.adapter_id,
            row.revision,
            request.expected_state.as_str(),
            row.state,
            request.event,
        )
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        row_to_record(row)
    }

    pub async fn history(
        &self,
        tenant_id: Uuid,
        adapter_id: &str,
    ) -> Result<Vec<BridgeAdapterTransitionRecord>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, tenant_id, adapter_id, revision, from_state, to_state, event, created_at
            FROM bridge_adapter_transition
            WHERE tenant_id = $1 AND adapter_id = $2
            ORDER BY revision
            "#,
            tenant_id,
            adapter_id,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(BridgeAdapterTransitionRecord {
                    id: row.id,
                    tenant_id: row.tenant_id,
                    adapter_id: row.adapter_id,
                    revision: row.revision,
                    from_state: row
                        .from_state
                        .as_deref()
                        .map(BridgeAdapterState::try_from)
                        .transpose()?,
                    to_state: BridgeAdapterState::try_from(row.to_state.as_str())?,
                    event: row.event,
                    created_at: row.created_at,
                })
            })
            .collect()
    }
}

fn row_to_record(row: BridgeAdapterRow) -> Result<BridgeAdapterRecord, StoreError> {
    Ok(BridgeAdapterRecord {
        id: row.id,
        tenant_id: row.tenant_id,
        adapter_id: row.adapter_id,
        component_ref: row.component_ref,
        component_sha256: row.component_sha256,
        grant_config: row.grant_config,
        config: row.config,
        descriptor: row.descriptor,
        state: BridgeAdapterState::try_from(row.state.as_str())?,
        last_error: row.last_error,
        revision: row.revision,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
