use sqlx::PgPool;
use uuid::Uuid;

use crate::StoreError;

#[derive(Clone)]
pub struct AdapterKvRepo {
    pool: PgPool,
}

impl AdapterKvRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn get_for_tenant(
        &self,
        tenant_id: Uuid,
        adapter_id: &str,
        key: &str,
    ) -> Result<Option<String>, StoreError> {
        validate_scope(tenant_id, adapter_id, key, None)?;
        let row = sqlx::query!(
            r#"
            SELECT v
            FROM tenant_adapter_kv
            WHERE tenant_id = $1 AND adapter_id = $2 AND k = $3
            "#,
            tenant_id,
            adapter_id,
            key
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| row.v))
    }

    pub async fn set_for_tenant(
        &self,
        tenant_id: Uuid,
        adapter_id: &str,
        key: &str,
        value: &str,
    ) -> Result<(), StoreError> {
        validate_scope(tenant_id, adapter_id, key, Some(value))?;
        sqlx::query!(
            r#"
            INSERT INTO tenant_adapter_kv (tenant_id, adapter_id, k, v)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (tenant_id, adapter_id, k)
            DO UPDATE SET v = EXCLUDED.v
            "#,
            tenant_id,
            adapter_id,
            key,
            value
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get(&self, adapter_id: &str, key: &str) -> Result<Option<String>, StoreError> {
        let _ = (adapter_id, key);
        Err(StoreError::Invariant(
            "tenant-scoped adapter KV API required".to_owned(),
        ))
    }

    pub async fn set(&self, adapter_id: &str, key: &str, value: &str) -> Result<(), StoreError> {
        let _ = (adapter_id, key, value);
        Err(StoreError::Invariant(
            "tenant-scoped adapter KV API required".to_owned(),
        ))
    }
}

fn validate_scope(
    tenant_id: Uuid,
    adapter_id: &str,
    key: &str,
    value: Option<&str>,
) -> Result<(), StoreError> {
    if tenant_id.is_nil() {
        return Err(StoreError::Invariant(
            "adapter KV tenant ID must not be nil".to_owned(),
        ));
    }
    validate_text("adapter ID", adapter_id, 128)?;
    validate_text("adapter KV key", key, 256)?;
    if let Some(value) = value {
        if value.len() > 65_536 {
            return Err(StoreError::Invariant(
                "adapter KV value exceeds 65536 bytes".to_owned(),
            ));
        }
        if value.contains('\0') {
            return Err(StoreError::Invariant(
                "adapter KV value contains a NUL byte".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, max_bytes: usize) -> Result<(), StoreError> {
    if value.trim().is_empty() {
        return Err(StoreError::Invariant(format!("{label} must not be blank")));
    }
    if value.len() > max_bytes {
        return Err(StoreError::Invariant(format!(
            "{label} exceeds {max_bytes} bytes"
        )));
    }
    if value.contains('\0') {
        return Err(StoreError::Invariant(format!(
            "{label} contains a NUL byte"
        )));
    }
    Ok(())
}
