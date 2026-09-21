use sqlx::PgPool;

use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub retry_after_secs: u64,
}

#[derive(Clone)]
pub struct RateLimitsRepo {
    pool: PgPool,
}

impl RateLimitsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn check(
        &self,
        key_digest: &str,
        max_requests: i64,
        window_secs: i64,
    ) -> Result<RateLimitDecision, StoreError> {
        validate_key_digest(key_digest)?;
        validate_window(max_requests, window_secs)?;

        let row = sqlx::query!(
            r#"
            WITH updated AS (
                INSERT INTO rate_limit_window (
                    key_digest,
                    window_started_at,
                    request_count
                )
                VALUES ($1, now(), 1)
                ON CONFLICT (key_digest) DO UPDATE
                SET window_started_at = CASE
                        WHEN rate_limit_window.window_started_at <=
                            now() - ($2::double precision * interval '1 second')
                        THEN now()
                        ELSE rate_limit_window.window_started_at
                    END,
                    request_count = CASE
                        WHEN rate_limit_window.window_started_at <=
                            now() - ($2::double precision * interval '1 second')
                        THEN 1
                        WHEN rate_limit_window.request_count <= $3
                        THEN rate_limit_window.request_count + 1
                        ELSE rate_limit_window.request_count
                    END
                RETURNING window_started_at, request_count
            )
            SELECT
                request_count <= $3 AS "allowed!",
                GREATEST(
                    1::BIGINT,
                    CEIL(EXTRACT(EPOCH FROM (
                        window_started_at
                        + ($2::double precision * interval '1 second')
                        - now()
                    )))::BIGINT
                ) AS "retry_after_secs!"
            FROM updated
            "#,
            key_digest,
            window_secs as f64,
            max_requests,
        )
        .fetch_one(&self.pool)
        .await?;

        let retry_after_secs = u64::try_from(row.retry_after_secs).map_err(|_| {
            StoreError::Invariant("rate-limit retry-after value was negative".to_owned())
        })?;
        Ok(RateLimitDecision {
            allowed: row.allowed,
            retry_after_secs,
        })
    }

    pub async fn prune_expired(&self, retention_secs: i64) -> Result<u64, StoreError> {
        if retention_secs <= 0 {
            return Err(StoreError::Invariant(
                "rate-limit retention must be positive".to_owned(),
            ));
        }
        let result = sqlx::query!(
            r#"
            DELETE FROM rate_limit_window
            WHERE window_started_at <
                now() - ($1::double precision * interval '1 second')
            "#,
            retention_secs as f64,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

fn validate_key_digest(key_digest: &str) -> Result<(), StoreError> {
    if key_digest.len() != 64
        || !key_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(StoreError::Invariant(
            "rate-limit key must be a lowercase SHA-256 digest".to_owned(),
        ));
    }
    Ok(())
}

fn validate_window(max_requests: i64, window_secs: i64) -> Result<(), StoreError> {
    if max_requests <= 0 || window_secs <= 0 {
        return Err(StoreError::Invariant(
            "rate-limit maximum and window must be positive".to_owned(),
        ));
    }
    Ok(())
}
