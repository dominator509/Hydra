use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::StoreError;

#[derive(Debug, Clone)]
pub struct LedgerRow {
    pub ts: OffsetDateTime,
    pub tenant_id: Uuid,
    pub route: String,
    pub provider: String,
    pub prefix_sha: String,
    pub hit_tokens: i32,
    pub miss_tokens: i32,
    pub out_tokens: i32,
    pub out_bytes: i32,
    pub aborted: bool,
    pub cost_cents: i32,
}

#[derive(Clone)]
pub struct LedgerRepo {
    pool: PgPool,
}

impl LedgerRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn record(&self, row: &LedgerRow) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            INSERT INTO tk_ledger (
                ts,
                tenant_id,
                route,
                provider,
                prefix_sha,
                hit_tokens,
                miss_tokens,
                out_tokens,
                out_bytes,
                aborted,
                cost_cents
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            row.ts,
            row.tenant_id,
            row.route,
            row.provider,
            row.prefix_sha,
            row.hit_tokens,
            row.miss_tokens,
            row.out_tokens,
            row.out_bytes,
            row.aborted,
            row.cost_cents,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn route_ratio(
        &self,
        route: &str,
        since: OffsetDateTime,
    ) -> Result<Option<f64>, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT
                CASE
                    WHEN COALESCE(SUM(hit_tokens + miss_tokens), 0) = 0 THEN NULL
                    ELSE SUM(hit_tokens)::float8 / SUM(hit_tokens + miss_tokens)::float8
                END AS "ratio?"
            FROM tk_ledger
            WHERE route = $1
              AND ts >= $2
            "#,
            route,
            since,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.ratio)
    }

    pub async fn month_to_date_cents(
        &self,
        tenant: Uuid,
        since: OffsetDateTime,
    ) -> Result<u64, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT COALESCE(SUM(cost_cents), 0) AS "total!"
            FROM tk_ledger
            WHERE tenant_id = $1
              AND ts >= $2
            "#,
            tenant,
            since,
        )
        .fetch_one(&self.pool)
        .await?;

        u64::try_from(row.total).map_err(|_| {
            StoreError::Invariant(format!("negative spend total in ledger: {}", row.total))
        })
    }
}
