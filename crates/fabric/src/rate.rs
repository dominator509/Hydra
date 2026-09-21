use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header::RETRY_AFTER, Request},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Fixed-window rate limiter with an explicit local or Store-backed authority.
pub struct RateLimiter {
    /// Local-only state retained for deterministic unit and compatibility tests.
    windows: Mutex<HashMap<String, (Instant, u32)>>,
    max_requests: u32,
    window: Duration,
    store: Option<store::RateLimitsRepo>,
    last_prune: AtomicU64,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            max_requests,
            window: Duration::from_secs(window_secs),
            store: None,
            last_prune: AtomicU64::new(0),
        }
    }

    /// Construct the production limiter. Store failures never fall back to local state.
    pub fn with_store(store: store::Store, max_requests: u32, window_secs: u64) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            max_requests,
            window: Duration::from_secs(window_secs),
            store: Some(store.rate_limits),
            last_prune: AtomicU64::new(0),
        }
    }

    pub fn check(&self, key: &str) -> Result<(), RateLimitError> {
        let mut windows = self.windows.lock().map_err(|_| RateLimitError {
            kind: RateLimitErrorKind::BackendUnavailable,
        })?;
        let now = Instant::now();
        windows.retain(|_, (started_at, _)| now.duration_since(*started_at) < self.window);
        let entry = windows.entry(key.to_owned()).or_insert((now, 0));

        if now.duration_since(entry.0) >= self.window {
            *entry = (now, 1);
            Ok(())
        } else if entry.1 >= self.max_requests {
            let remaining = self.window.saturating_sub(now.duration_since(entry.0));
            Err(RateLimitError {
                kind: RateLimitErrorKind::Exceeded {
                    retry_after_secs: remaining.as_secs().max(1),
                },
            })
        } else {
            entry.1 += 1;
            Ok(())
        }
    }

    pub async fn check_async(&self, key: &str) -> Result<(), RateLimitError> {
        let Some(store) = &self.store else {
            return self.check(key);
        };

        let window_secs = i64::try_from(self.window.as_secs()).map_err(|_| RateLimitError {
            kind: RateLimitErrorKind::BackendUnavailable,
        })?;
        let now = unix_seconds();
        let previous = self.last_prune.load(Ordering::Relaxed);
        if now.saturating_sub(previous) >= 60
            && self
                .last_prune
                .compare_exchange(previous, now, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        {
            let retention_secs = window_secs.saturating_add(60);
            store
                .prune_expired(retention_secs)
                .await
                .map_err(|_| RateLimitError {
                    kind: RateLimitErrorKind::BackendUnavailable,
                })?;
        }

        let decision = store
            .check(&digest_key(key), i64::from(self.max_requests), window_secs)
            .await
            .map_err(|_| RateLimitError {
                kind: RateLimitErrorKind::BackendUnavailable,
            })?;
        if decision.allowed {
            Ok(())
        } else {
            Err(RateLimitError {
                kind: RateLimitErrorKind::Exceeded {
                    retry_after_secs: decision.retry_after_secs.max(1),
                },
            })
        }
    }
}

#[derive(Debug)]
pub struct RateLimitError {
    kind: RateLimitErrorKind,
}

#[derive(Debug)]
enum RateLimitErrorKind {
    Exceeded { retry_after_secs: u64 },
    BackendUnavailable,
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        match self.kind {
            RateLimitErrorKind::Exceeded { retry_after_secs } => (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                [
                    (
                        axum::http::header::CONTENT_TYPE,
                        "application/problem+json".to_owned(),
                    ),
                    (RETRY_AFTER, retry_after_secs.to_string()),
                ],
                axum::Json(json!({
                    "type": "https://hydra.dev/errors/rate-limited",
                    "title": "Too Many Requests",
                    "status": 429,
                    "detail": "Rate limit exceeded. Please retry after the window resets."
                })),
            )
                .into_response(),
            RateLimitErrorKind::BackendUnavailable => (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                [(axum::http::header::CONTENT_TYPE, "application/problem+json")],
                axum::Json(json!({
                    "type": "https://hydra.dev/errors/rate-limit-unavailable",
                    "title": "Rate Limiter Unavailable",
                    "status": 503,
                    "detail": "Request admission is temporarily unavailable."
                })),
            )
                .into_response(),
        }
    }
}

pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, RateLimitError> {
    let key = request
        .extensions()
        .get::<crate::auth::PrincipalContext>()
        .map(|principal| {
            format!(
                "principal:{}:{}",
                principal.hydra_tenant_id, principal.principal_id
            )
        })
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ConnectInfo(address)| format!("network:{}", address.ip()))
        })
        .unwrap_or_else(|| "network:unattributed".to_owned());
    limiter.check_async(&key).await?;
    Ok(next.run(request).await)
}

fn digest_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"hydra-rate-limit:v1:");
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use sqlx::postgres::PgPoolOptions;

    #[test]
    fn capability_rate_limiter_rejects_after_configured_limit() {
        let limiter = RateLimiter::new(2, 60);
        assert!(limiter.check("principal-a").is_ok());
        assert!(limiter.check("principal-a").is_ok());
        assert!(limiter.check("principal-a").is_err());
    }

    #[test]
    fn capability_rate_limiter_isolates_keys() {
        let limiter = RateLimiter::new(1, 60);
        assert!(limiter.check("principal-a").is_ok());
        assert!(limiter.check("principal-a").is_err());
        assert!(limiter.check("principal-b").is_ok());
    }

    #[test]
    fn digest_is_versioned_and_does_not_disclose_the_raw_key() {
        let raw = "principal:tenant-a:user-a";
        let digest = digest_key(raw);
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(!digest.contains(raw));
        assert_ne!(digest, digest_key("principal:tenant-b:user-a"));
    }

    #[tokio::test]
    async fn store_failure_fails_closed_with_generic_503() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://hydra:hydra@127.0.0.1:1/hydra")
            .expect("lazy pool construction");
        let limiter = RateLimiter::with_store(store::Store::new(pool), 1, 60);
        let error = limiter
            .check_async("principal:tenant-a:user-a")
            .await
            .expect_err("unavailable authority must fail closed");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
