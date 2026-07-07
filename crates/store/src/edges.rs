use cdm::Edge;
use sqlx::PgPool;

use crate::StoreError;

#[derive(Clone)]
pub struct EdgesRepo {
    pool: PgPool,
}

impl EdgesRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, edge: Edge) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            INSERT INTO edge (src, rel, dst)
            VALUES ($1, $2, $3)
            ON CONFLICT (src, rel, dst) DO NOTHING
            "#,
            edge.src,
            edge.rel,
            edge.dst,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn attach_body(
        &self,
        edge: &Edge,
        body: serde_json::Value,
    ) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            UPDATE edge
            SET body = $4
            WHERE src = $1 AND rel = $2 AND dst = $3
            "#,
            edge.src,
            edge.rel,
            edge.dst,
            body
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
