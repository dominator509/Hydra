use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use axum::{
    body::Body,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

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
        let mut windows = self.windows.lock().expect("rate limiter lock");
        let now = Instant::now();
        let entry = windows.entry(key.to_owned()).or_insert((now, 0));

        if now.duration_since(entry.0) > self.window {
            *entry = (now, 1);
            Ok(())
        } else if entry.1 >= self.max_requests {
            Err(RateLimitError)
        } else {
            entry.1 += 1;
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct RateLimitError;

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::CONTENT_TYPE, "application/problem+json")],
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
    request: Request<Body>,
    next: Next,
) -> Result<Response, RateLimitError> {
    // Extract client identifier (IP or x-forwarded-for)
    let client_ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    // This would use a shared RateLimiter — for now, always allow
    let _ = client_ip;
    Ok(next.run(request).await)
}
