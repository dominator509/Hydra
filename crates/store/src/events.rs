use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::StoreError;

#[derive(Clone)]
pub struct EventsRepo {
    pool: PgPool,
}

impl EventsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn append(
        tx: &mut Transaction<'_, Postgres>,
        tenant: Uuid,
        actor: &str,
        kind: &str,
        payload: &Value,
    ) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            INSERT INTO event_log (tenant_id, actor, kind, payload)
            VALUES ($1, $2, $3, $4)
            "#,
            tenant,
            actor,
            kind,
            payload.clone()
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}
