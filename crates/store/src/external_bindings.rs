use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingStatus {
    Active,
    Disabled,
    Revoked,
}

impl BindingStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Revoked => "revoked",
        }
    }
}

impl TryFrom<&str> for BindingStatus {
    type Error = StoreError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "active" => Ok(Self::Active),
            "disabled" => Ok(Self::Disabled),
            "revoked" => Ok(Self::Revoked),
            other => Err(StoreError::Invariant(format!(
                "unknown external binding status '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalTenantBinding {
    pub id: Uuid,
    pub provider: String,
    pub external_tenant_id: String,
    pub external_business_id: String,
    pub hydra_tenant_id: Uuid,
    pub status: BindingStatus,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewExternalTenantBinding {
    pub provider: String,
    pub external_tenant_id: String,
    pub external_business_id: String,
    pub hydra_tenant_id: Uuid,
}

impl NewExternalTenantBinding {
    fn validate(&self) -> Result<(), StoreError> {
        for (name, value) in [
            ("provider", self.provider.as_str()),
            ("external_tenant_id", self.external_tenant_id.as_str()),
            ("external_business_id", self.external_business_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(StoreError::Invariant(format!(
                    "external binding {name} cannot be empty"
                )));
            }
        }
        if self.hydra_tenant_id.is_nil() {
            return Err(StoreError::Invariant(
                "external binding hydra_tenant_id cannot be nil".to_owned(),
            ));
        }
        Ok(())
    }
}

struct BindingRow {
    id: Uuid,
    provider: String,
    external_tenant_id: String,
    external_business_id: String,
    hydra_tenant_id: Uuid,
    status: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(Clone)]
pub struct ExternalBindingsRepo {
    pool: PgPool,
}

impl ExternalBindingsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        binding: NewExternalTenantBinding,
    ) -> Result<ExternalTenantBinding, StoreError> {
        binding.validate()?;
        let row = sqlx::query_as!(
            BindingRow,
            r#"
            INSERT INTO external_tenant_binding (
                provider,
                external_tenant_id,
                external_business_id,
                hydra_tenant_id
            )
            VALUES ($1, $2, $3, $4)
            RETURNING
                id,
                provider,
                external_tenant_id,
                external_business_id,
                hydra_tenant_id,
                status,
                created_at,
                updated_at
            "#,
            binding.provider,
            binding.external_tenant_id,
            binding.external_business_id,
            binding.hydra_tenant_id,
        )
        .fetch_one(&self.pool)
        .await?;

        row_to_binding(row)
    }

    pub async fn resolve(
        &self,
        provider: &str,
        external_tenant_id: &str,
        external_business_id: &str,
    ) -> Result<Option<ExternalTenantBinding>, StoreError> {
        let row = sqlx::query_as!(
            BindingRow,
            r#"
            SELECT
                id,
                provider,
                external_tenant_id,
                external_business_id,
                hydra_tenant_id,
                status,
                created_at,
                updated_at
            FROM external_tenant_binding
            WHERE provider = $1
              AND external_tenant_id = $2
              AND external_business_id = $3
            "#,
            provider,
            external_tenant_id,
            external_business_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_binding).transpose()
    }

    pub async fn list_for_hydra_tenant(
        &self,
        hydra_tenant_id: Uuid,
    ) -> Result<Vec<ExternalTenantBinding>, StoreError> {
        let rows = sqlx::query_as!(
            BindingRow,
            r#"
            SELECT
                id,
                provider,
                external_tenant_id,
                external_business_id,
                hydra_tenant_id,
                status,
                created_at,
                updated_at
            FROM external_tenant_binding
            WHERE hydra_tenant_id = $1
            ORDER BY provider, external_tenant_id, external_business_id
            "#,
            hydra_tenant_id,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_binding).collect()
    }

    pub async fn set_status(
        &self,
        id: Uuid,
        status: BindingStatus,
    ) -> Result<ExternalTenantBinding, StoreError> {
        let row = sqlx::query_as!(
            BindingRow,
            r#"
            UPDATE external_tenant_binding
            SET status = $2,
                updated_at = now()
            WHERE id = $1
            RETURNING
                id,
                provider,
                external_tenant_id,
                external_business_id,
                hydra_tenant_id,
                status,
                created_at,
                updated_at
            "#,
            id,
            status.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_binding)
            .transpose()?
            .ok_or(StoreError::NotFound)
    }
}

fn row_to_binding(row: BindingRow) -> Result<ExternalTenantBinding, StoreError> {
    Ok(ExternalTenantBinding {
        id: row.id,
        provider: row.provider,
        external_tenant_id: row.external_tenant_id,
        external_business_id: row.external_business_id,
        hydra_tenant_id: row.hydra_tenant_id,
        status: BindingStatus::try_from(row.status.as_str())?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
