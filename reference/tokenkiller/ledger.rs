//! reference/tokenkiller/ledger.rs — usage accounting + hit-ratio SLO (SPEC-009 TK7).
//! Adapted into crates/tokenkiller (types + math) and crates/store (persistence, table
//! tk_ledger per SPEC-002). Providers report cache usage via the trait below; DeepSeek
//! maps prompt_cache_hit_tokens/prompt_cache_miss_tokens; others report miss=all.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheUsage { pub hit_tokens: u64, pub miss_tokens: u64 }

impl CacheUsage {
    /// DeepSeek response usage → CacheUsage. Field names per ASSUMPTION A3;
    /// verify once in EP-003 M4 probe and keep this the ONLY parse site.
    pub fn from_deepseek_usage(u: &serde_json::Value) -> Self {
        let g = |k: &str| u.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
        let hit = g("prompt_cache_hit_tokens");
        let mut miss = g("prompt_cache_miss_tokens");
        if hit == 0 && miss == 0 {
            miss = g("prompt_tokens"); // non-caching fallback: everything billed as miss
        }
        Self { hit_tokens: hit, miss_tokens: miss }
    }
}

#[derive(Debug, Clone)]
pub struct LedgerRow {
    pub ts_unix: i64,
    pub tenant: uuid::Uuid,
    pub route: String,
    pub provider: String,
    pub prefix_sha_hex: String, // forensics key: ratio cliffs align with sha changes
    pub usage: CacheUsage,
    pub out_tokens: u64,
    pub out_bytes: u64,
    pub aborted: bool,          // NukeGuard tripped on this call
    pub cost_cents: u32,
}

/// Rolling ratio over rows already filtered to (route, window).
/// SLO: >= 0.97 on deepseek routes; alert thresholds in OBSERVABILITY.md.
pub fn hit_ratio<'a>(rows: impl IntoIterator<Item = &'a LedgerRow>) -> Option<f64> {
    let (mut hit, mut total) = (0u128, 0u128);
    for r in rows {
        hit += r.usage.hit_tokens as u128;
        total += (r.usage.hit_tokens + r.usage.miss_tokens) as u128;
    }
    (total > 0).then(|| hit as f64 / total as f64)
}

/// Month-to-date spend feeds the Governor constitution (SPEC-009 TK9).
pub fn month_to_date_cents<'a>(rows: impl IntoIterator<Item = &'a LedgerRow>) -> u64 {
    rows.into_iter().map(|r| r.cost_cents as u64).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(hit: u64, miss: u64) -> LedgerRow {
        LedgerRow { ts_unix: 0, tenant: uuid::Uuid::nil(), route: "concierge".into(),
            provider: "deepseek".into(), prefix_sha_hex: "aa".into(),
            usage: CacheUsage { hit_tokens: hit, miss_tokens: miss },
            out_tokens: 0, out_bytes: 0, aborted: false, cost_cents: 1 }
    }
    #[test]
    fn tk_ratio_math() {
        let rows = [row(0, 3650), row(3520, 130), row(3584, 128), row(3648, 130)];
        let r = hit_ratio(rows.iter()).unwrap();
        assert!(r > 0.72 && r < 0.75, "cold first call drags the small-N window: {r}");
        let warm = hit_ratio(rows[1..].iter()).unwrap();
        assert!(warm >= 0.96, "steady state must clear the SLO shoulder: {warm}");
    }
    #[test]
    fn tk_deepseek_usage_parse_and_fallback() {
        let u = serde_json::json!({"prompt_cache_hit_tokens": 3520, "prompt_cache_miss_tokens": 130});
        assert_eq!(CacheUsage::from_deepseek_usage(&u), CacheUsage { hit_tokens: 3520, miss_tokens: 130 });
        let plain = serde_json::json!({"prompt_tokens": 900});
        assert_eq!(CacheUsage::from_deepseek_usage(&plain), CacheUsage { hit_tokens: 0, miss_tokens: 900 });
    }
}
