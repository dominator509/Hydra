use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheUsage {
    pub hit_tokens: u64,
    pub miss_tokens: u64,
}

impl CacheUsage {
    pub fn from_deepseek_usage(value: &serde_json::Value) -> Self {
        let get = |key: &str| value.get(key).and_then(|field| field.as_u64()).unwrap_or(0);
        let hit = get("prompt_cache_hit_tokens");
        let mut miss = get("prompt_cache_miss_tokens");
        if hit == 0 && miss == 0 {
            miss = get("prompt_tokens");
        }
        Self {
            hit_tokens: hit,
            miss_tokens: miss,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LedgerRow {
    pub ts: OffsetDateTime,
    pub tenant_id: Uuid,
    pub route: String,
    pub provider: String,
    pub prefix_sha: String,
    pub usage: CacheUsage,
    pub out_tokens: u64,
    pub out_bytes: u64,
    pub aborted: bool,
    pub cost_cents: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("ledger persistence failed: {0}")]
    Persist(String),
    #[error("ledger field '{field}' exceeds store integer range: {value}")]
    Overflow { field: &'static str, value: u64 },
}

#[async_trait]
pub trait LedgerSink: Send + Sync {
    async fn record(&self, row: &LedgerRow) -> Result<(), LedgerError>;
}

#[derive(Clone)]
pub struct StoreLedgerSink {
    repo: store::LedgerRepo,
}

impl StoreLedgerSink {
    pub fn new(repo: store::LedgerRepo) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl LedgerSink for StoreLedgerSink {
    async fn record(&self, row: &LedgerRow) -> Result<(), LedgerError> {
        self.repo
            .record(&to_store_row(row)?)
            .await
            .map_err(|error| LedgerError::Persist(error.to_string()))
    }
}

pub fn hit_ratio<'a>(rows: impl IntoIterator<Item = &'a LedgerRow>) -> Option<f64> {
    let (mut hit, mut total) = (0_u128, 0_u128);
    for row in rows {
        hit += u128::from(row.usage.hit_tokens);
        total += u128::from(row.usage.hit_tokens + row.usage.miss_tokens);
    }
    (total > 0).then(|| hit as f64 / total as f64)
}

pub fn month_to_date_cents<'a>(rows: impl IntoIterator<Item = &'a LedgerRow>) -> u64 {
    rows.into_iter().map(|row| u64::from(row.cost_cents)).sum()
}

fn to_store_row(row: &LedgerRow) -> Result<store::LedgerRow, LedgerError> {
    Ok(store::LedgerRow {
        ts: row.ts,
        tenant_id: row.tenant_id,
        route: row.route.clone(),
        provider: row.provider.clone(),
        prefix_sha: row.prefix_sha.clone(),
        hit_tokens: as_i32("hit_tokens", row.usage.hit_tokens)?,
        miss_tokens: as_i32("miss_tokens", row.usage.miss_tokens)?,
        out_tokens: as_i32("out_tokens", row.out_tokens)?,
        out_bytes: as_i32("out_bytes", row.out_bytes)?,
        aborted: row.aborted,
        cost_cents: as_i32("cost_cents", u64::from(row.cost_cents))?,
    })
}

fn as_i32(field: &'static str, value: u64) -> Result<i32, LedgerError> {
    i32::try_from(value).map_err(|_| LedgerError::Overflow { field, value })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(hit: u64, miss: u64) -> LedgerRow {
        LedgerRow {
            ts: OffsetDateTime::UNIX_EPOCH,
            tenant_id: Uuid::nil(),
            route: "concierge".into(),
            provider: "deepseek".into(),
            prefix_sha: "aa".into(),
            usage: CacheUsage {
                hit_tokens: hit,
                miss_tokens: miss,
            },
            out_tokens: 0,
            out_bytes: 0,
            aborted: false,
            cost_cents: 1,
        }
    }

    #[test]
    fn tk_ratio_math() {
        let rows = [row(0, 3650), row(3520, 130), row(3584, 128), row(3648, 130)];
        let ratio = hit_ratio(rows.iter()).expect("rows should yield a ratio");
        assert!(
            ratio > 0.72 && ratio < 0.75,
            "cold first call drags the small-N window: {ratio}"
        );
        let warm = hit_ratio(rows[1..].iter()).expect("warm rows should yield a ratio");
        assert!(
            warm >= 0.96,
            "steady state must clear the SLO shoulder: {warm}"
        );
    }

    #[test]
    fn tk_deepseek_usage_parse_and_fallback() {
        let cached =
            serde_json::json!({"prompt_cache_hit_tokens": 3520, "prompt_cache_miss_tokens": 130});
        assert_eq!(
            CacheUsage::from_deepseek_usage(&cached),
            CacheUsage {
                hit_tokens: 3520,
                miss_tokens: 130,
            }
        );

        let plain = serde_json::json!({"prompt_tokens": 900});
        assert_eq!(
            CacheUsage::from_deepseek_usage(&plain),
            CacheUsage {
                hit_tokens: 0,
                miss_tokens: 900,
            }
        );
    }
}
