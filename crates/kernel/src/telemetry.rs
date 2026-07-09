//! EP-008: JSON-structured telemetry with field-level redaction.
//!
//! Uses a custom `Layer` (the Layer trait from tracing-subscriber) to intercept
//! every tracing event, visit its fields with a redacting visitor, and emit a
//! JSON line to stdout. Sensitive fields (password, secret, token, api_key,
//! prompt, tail) are masked as `"***"`. Safe SHA hash fields (prefix_sha,
//! tail_sha) are always allowed through.

use std::io::Write;
use time::OffsetDateTime;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer;
use tracing_subscriber::Registry;

/// Field names whose values MUST be redacted from logs.
const REDACTED: &[&str] = &["password", "secret", "token", "api_key", "prompt", "tail"];

/// Initialise the global telemetry subscriber.
///
/// Composes `JsonRedactLayer` (redaction + JSON output) with an `EnvFilter`
/// layer so that log level can be controlled via `RUST_LOG`.
pub fn init_telemetry() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = Registry::default()
        .with(JsonRedactLayer)
        .with(env_filter);

    tracing::subscriber::set_global_default(subscriber)
        .expect("global tracing subscriber is already set");
}

// ---------------------------------------------------------------------------
// Core event formatter (testable independently of the Layer)
// ---------------------------------------------------------------------------

/// Redact an event's fields and write a JSON line to `writer`.
///
/// Called by `JsonRedactLayer::on_event` (with stdout) and by tests (with a
/// `Vec<u8>` buffer).  This is the single function that defines the redaction
/// behaviour.
fn write_redacted_event(
    event: &tracing::Event<'_>,
    writer: &mut dyn Write,
) -> std::io::Result<()> {
    let meta = event.metadata();

    let mut fields = serde_json::Map::new();

    fields.insert(
        "ts".into(),
        serde_json::Value::String(iso_timestamp()),
    );
    fields.insert(
        "level".into(),
        serde_json::Value::String(format_level(meta.level()).to_string()),
    );
    fields.insert(
        "target".into(),
        serde_json::Value::String(meta.target().to_string()),
    );

    let mut visitor = FieldCollector(&mut fields);
    event.record(&mut visitor);

    // Promote "message" from the field bag to a top-level key so the JSON
    // output reads naturally.
    if let Some(msg) = fields.remove("message") {
        fields.insert("message".into(), msg);
    }

    let line = serde_json::to_string(&fields).unwrap_or_default();
    writeln!(writer, "{line}")
}

// ---------------------------------------------------------------------------
// Layer implementation
// ---------------------------------------------------------------------------

/// A tracing-subscriber [`Layer`] that visits every event's fields, redacts
/// sensitive values, and writes a JSON line to stdout.
struct JsonRedactLayer;

impl<S: tracing::Subscriber> Layer<S> for JsonRedactLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let _ = write_redacted_event(event, &mut std::io::stdout().lock());
    }
}

// ---------------------------------------------------------------------------
// Field visitor (implements tracing::field::Visit)
// ---------------------------------------------------------------------------

/// Visits every field of a tracing event, inserting the (redacted) value into
/// a JSON map.  Fields whose name appears in [`REDACTED`] get `"***"`.
struct FieldCollector<'a>(&'a mut serde_json::Map<String, serde_json::Value>);

impl tracing::field::Visit for FieldCollector<'_> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        let name = field.name();
        let val = if REDACTED.contains(&name) {
            serde_json::Value::String("***".to_string())
        } else {
            serde_json::Value::String(value.to_string())
        };
        self.0.insert(name.to_string(), val);
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0
            .insert(field.name().to_string(), serde_json::Value::Number(value.into()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0
            .insert(field.name().to_string(), serde_json::Value::Number(value.into()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0
            .insert(field.name().to_string(), serde_json::Value::Bool(value));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        if let Some(n) = serde_json::Number::from_f64(value) {
            self.0
                .insert(field.name().to_string(), serde_json::Value::Number(n));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let name = field.name();
        let val = if REDACTED.contains(&name) {
            serde_json::Value::String("***".to_string())
        } else {
            serde_json::Value::String(format!("{value:?}"))
        };
        self.0.insert(name.to_string(), val);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn iso_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".into())
}

fn format_level(level: &tracing::metadata::Level) -> &'static str {
    match level {
        &tracing::Level::ERROR => "ERROR",
        &tracing::Level::WARN => "WARN",
        &tracing::Level::INFO => "INFO",
        &tracing::Level::DEBUG => "DEBUG",
        &tracing::Level::TRACE => "TRACE",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A test Layer that captures redacted output into a shared Vec.
    struct CaptureLayer(Arc<Mutex<Vec<String>>>);

    impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut buf: Vec<u8> = Vec::new();
            if write_redacted_event(event, &mut buf).is_ok() {
                if let Ok(s) = String::from_utf8(buf) {
                    self.0.lock().unwrap().push(s);
                }
            }
        }
    }

    /// Build a subscriber that uses [`CaptureLayer`] instead of stdout, so we
    /// can assert on the redacted JSON output.
    fn test_subscriber(captured: Arc<Mutex<Vec<String>>>) -> impl tracing::Subscriber {
        Registry::default()
            .with(CaptureLayer(captured))
            .with(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("info")),
            )
    }

    #[test]
    fn redaction_masks_secret_field() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let subscriber = test_subscriber(captured.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(secret = "s3cret-value!", "test log");

        let lines = captured.lock().unwrap();
        let json = lines.join("\n");

        assert!(
            !json.contains("s3cret-value!"),
            "secret field value leaked into log output:\n{json}"
        );
        assert!(
            json.contains("***"),
            "secret field was not masked with ***:\n{json}"
        );
    }

    #[test]
    fn redaction_masks_password_field() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let subscriber = test_subscriber(captured.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(password = "hunter2", "login attempt");

        let lines = captured.lock().unwrap();
        let json = lines.join("\n");

        assert!(!json.contains("hunter2"), "password leaked:\n{json}");
        assert!(json.contains("***"), "password not masked:\n{json}");
    }

    #[test]
    fn redaction_allows_prefix_sha() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let subscriber = test_subscriber(captured.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(prefix_sha = "abc123def456", "segment check");

        let lines = captured.lock().unwrap();
        let json = lines.join("\n");

        assert!(
            json.contains("abc123def456"),
            "prefix_sha was incorrectly redacted:\n{json}"
        );
    }

    #[test]
    fn redaction_allows_tail_sha() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let subscriber = test_subscriber(captured.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(tail_sha = "xyz789", "segment tail check");

        let lines = captured.lock().unwrap();
        let json = lines.join("\n");

        assert!(
            json.contains("xyz789"),
            "tail_sha was incorrectly redacted:\n{json}"
        );
    }

    #[test]
    fn redaction_masks_token_field() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let subscriber = test_subscriber(captured.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::warn!(token = "eyJhbGciOiJIUzI1NiJ9", "auth attempt");

        let lines = captured.lock().unwrap();
        let json = lines.join("\n");

        assert!(
            !json.contains("eyJhbGciOiJIUzI1NiJ9"),
            "token leaked:\n{json}"
        );
        assert!(json.contains("***"), "token not masked:\n{json}");
    }

    #[test]
    fn redaction_masks_api_key() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let subscriber = test_subscriber(captured.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(api_key = "sk-abcdef123456", "llm call");

        let lines = captured.lock().unwrap();
        let json = lines.join("\n");

        assert!(!json.contains("sk-abcdef123456"), "api_key leaked:\n{json}");
        assert!(json.contains("***"), "api_key not masked:\n{json}");
    }

    #[test]
    fn redaction_masks_prompt_field() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let subscriber = test_subscriber(captured.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(prompt = "What is the meaning of life?", "user prompt");

        let lines = captured.lock().unwrap();
        let json = lines.join("\n");

        assert!(
            !json.contains("meaning of life"),
            "prompt content leaked:\n{json}"
        );
        assert!(json.contains("***"), "prompt not masked:\n{json}");
    }

    #[test]
    fn redaction_masks_tail_field() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let subscriber = test_subscriber(captured.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(tail = "sensitive-suffix", "segment tail");

        let lines = captured.lock().unwrap();
        let json = lines.join("\n");

        assert!(!json.contains("sensitive-suffix"), "tail leaked:\n{json}");
        assert!(json.contains("***"), "tail not masked:\n{json}");
    }
}
