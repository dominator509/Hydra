use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header::RETRY_AFTER, Request},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Token-bucket rate limiter keyed by IP/client identifier.
pub struct RateLimiter {
    /// Map from key -> (window_start, count)
    windows: Mutex<HashMap<String, (Instant, u32)>>,
    max_requests: u32,
    window: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            max_requests,
            window: Duration::from_secs(window_secs),
        }
    }

    pub fn check(&self, key: &str) -> Result<(), RateLimitError> {
        let mut windows = self.windows.lock().map_err(|_| RateLimitError {
            retry_after_secs: 1,
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
                retry_after_secs: remaining.as_secs().max(1),
            })
        } else {
            entry.1 += 1;
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct RateLimitError {
    retry_after_secs: u64,
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            [
                (
                    axum::http::header::CONTENT_TYPE,
                    "application/problem+json".to_owned(),
                ),
                (RETRY_AFTER, self.retry_after_secs.to_string()),
            ],
            axum::Json(json!({
                "type": "https://hydra.dev/errors/rate-limited",
                "title": "Too Many Requests",
                "status": 429,
                "detail": "Rate limit exceeded. Please retry after the window resets."
            })),
        )
            .into_response()
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
    limiter.check(&key)?;
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
