use sqlx::PgPool;

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

    pub async fn get(&self, adapter_id: &str, key: &str) -> Result<Option<String>, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT v
            FROM adapter_kv
            WHERE adapter_id = $1 AND k = $2
            "#,
            adapter_id,
            key
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| row.v))
    }

    pub async fn set(&self, adapter_id: &str, key: &str, value: &str) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            INSERT INTO adapter_kv (adapter_id, k, v)
            VALUES ($1, $2, $3)
            ON CONFLICT (adapter_id, k)
            DO UPDATE SET v = EXCLUDED.v
            "#,
            adapter_id,
            key,
            value
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
