//! EP-008: In-memory Prometheus metrics registry and `/metrics` handler.
//!
//! Exposes counters, gauges, and histograms via a global registry.  The
//! `/metrics` endpoint renders them in Prometheus text format for scraping.

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{Arc, LazyLock, Mutex};

use axum::response::IntoResponse;
use axum::{body::Body, extract::Request, http::StatusCode, middleware::Next, response::Response};

// ---------------------------------------------------------------------------
// Default histogram buckets (seconds, matching Prometheus default).
// ---------------------------------------------------------------------------
const DURATION_BUCKETS: &[f64] = &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0];

// ---------------------------------------------------------------------------
// Global registry (lazily initialised once on first access).
// ---------------------------------------------------------------------------

static REGISTRY: LazyLock<MetricsRegistry> = LazyLock::new(MetricsRegistry::new);

/// Return a reference to the global metrics registry.
pub(crate) fn registry() -> &'static MetricsRegistry {
    &REGISTRY
}

// ---------------------------------------------------------------------------
// Metric value types (internal)
// ---------------------------------------------------------------------------

/// A counter with an optional label set.
#[derive(Clone, Debug)]
struct CounterRow {
    labels: Vec<(String, String)>,
    value: u64,
}

/// A histogram bucket set for a single label combination.
#[derive(Clone, Debug)]
struct HistogramRow {
    labels: Vec<(String, String)>,
    /// (upper_bound, cumulative_count) — +Inf is implicit after the last entry.
    buckets: Vec<(f64, u64)>,
    sum: f64,
    count: u64,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Thread-safe, in-memory metrics registry.
pub(crate) struct MetricsRegistry {
    counters: Arc<Mutex<HashMap<String, CounterFamily>>>,
    histograms: Arc<Mutex<HashMap<String, HistogramFamily>>>,
    gauges: Arc<Mutex<HashMap<String, GaugeValue>>>,
}

#[derive(Clone, Debug)]
struct CounterFamily {
    help: String,
    rows: Vec<CounterRow>,
}

#[derive(Clone, Debug)]
struct HistogramFamily {
    help: String,
    rows: Vec<HistogramRow>,
}

#[derive(Clone, Debug)]
struct GaugeValue {
    help: String,
    value: f64,
}

impl MetricsRegistry {
    fn new() -> Self {
        let mut reg = Self {
            counters: Arc::new(Mutex::new(HashMap::new())),
            histograms: Arc::new(Mutex::new(HashMap::new())),
            gauges: Arc::new(Mutex::new(HashMap::new())),
        };
        reg.register_defaults();
        reg
    }

    /// Pre-register all metrics named in OBSERVABILITY.md so they always
    /// appear in the /metrics output, even before their first recording.
    fn register_defaults(&mut self) {
        self.counters
            .lock()
            .expect("metrics counters lock should not be poisoned")
            .entry("hydra_requests_total".into())
            .or_insert_with(|| CounterFamily {
                help: "Total HTTP requests".into(),
                rows: vec![],
            });

        self.counters
            .lock()
            .expect("metrics counters lock should not be poisoned")
            .entry("hydra_envelopes_total".into())
            .or_insert_with(|| CounterFamily {
                help: "Envelopes by state".into(),
                rows: vec![],
            });

        self.counters
            .lock()
            .expect("metrics counters lock should not be poisoned")
            .entry("hydra_tk_nuke_aborts_total".into())
            .or_insert_with(|| CounterFamily {
                help: "Total TK nuke aborts".into(),
                rows: vec![],
            });

        self.counters
            .lock()
            .expect("metrics counters lock should not be poisoned")
            .entry("hydra_bridge_sync_scheduler_operations_total".into())
            .or_insert_with(|| CounterFamily {
                help: "Bridge sync scheduler operations by bounded outcome".into(),
                rows: vec![],
            });

        self.histograms
            .lock()
            .expect("metrics histograms lock should not be poisoned")
            .entry("hydra_request_duration_seconds".into())
            .or_insert_with(|| HistogramFamily {
                help: "Request duration distribution (seconds)".into(),
                rows: vec![],
            });

        self.gauges
            .lock()
            .expect("metrics gauges lock should not be poisoned")
            .entry("hydra_tk_cache_hit_ratio".into())
            .or_insert_with(|| GaugeValue {
                help: "TK cache hit ratio (1h rolling)".into(),
                value: 1.0,
            });

        self.gauges
            .lock()
            .expect("metrics gauges lock should not be poisoned")
            .entry("hydra_db_connections".into())
            .or_insert_with(|| GaugeValue {
                help: "Active database connections".into(),
                value: 0.0,
            });
    }

    // -- counters -----------------------------------------------------------

    pub(crate) fn inc_counter(&self, name: &str, labels: Vec<(String, String)>) {
        let Ok(mut counters) = self.counters.lock() else {
            return;
        };
        let Some(family) = counters.get_mut(name) else {
            return;
        };

        // Look for an existing row with the same labels.
        if let Some(row) = family.rows.iter_mut().find(|r| r.labels == labels) {
            row.value += 1;
        } else {
            family.rows.push(CounterRow { labels, value: 1 });
        }
    }

    // -- histograms ---------------------------------------------------------

    pub(crate) fn observe_histogram(&self, name: &str, value: f64, labels: Vec<(String, String)>) {
        let Ok(mut histos) = self.histograms.lock() else {
            return;
        };
        let Some(family) = histos.get_mut(name) else {
            return;
        };

        if let Some(row) = family.rows.iter_mut().find(|r| r.labels == labels) {
            row.count += 1;
            row.sum += value;
            // Increment the bucket whose upper bound is >= value.
            for (bound, count) in row.buckets.iter_mut() {
                if value <= *bound {
                    *count += 1;
                }
            }
        } else {
            let mut buckets: Vec<(f64, u64)> =
                DURATION_BUCKETS.iter().map(|b| (*b, 0u64)).collect();
            for (bound, count) in buckets.iter_mut() {
                if value <= *bound {
                    *count = 1;
                }
            }
            family.rows.push(HistogramRow {
                labels,
                buckets,
                sum: value,
                count: 1,
            });
        }
    }

    // -- gauges -------------------------------------------------------------

    #[cfg(test)]
    pub(crate) fn set_gauge(&self, name: &str, value: f64) {
        let mut gauges = self
            .gauges
            .lock()
            .expect("metrics gauges lock should not be poisoned");
        if let Some(g) = gauges.get_mut(name) {
            g.value = value;
        }
    }

    // -- render (Prometheus text format) ------------------------------------

    /// Render all metrics as a Prometheus text-format string.
    pub(crate) fn render(&self) -> String {
        let mut out = String::new();

        // Counters
        {
            let counters = self
                .counters
                .lock()
                .expect("metrics counters lock should not be poisoned");
            for (name, family) in counters.iter() {
                let _ = writeln!(out, "# HELP {name} {}", family.help);
                let _ = writeln!(out, "# TYPE {name} counter");
                for row in &family.rows {
                    write_metric_line(&mut out, name, &row.labels, row.value as f64);
                }
                // If no rows exist, emit a zero-valued bare line.
                if family.rows.is_empty() {
                    let _ = writeln!(out, "{name} 0");
                }
            }
        }

        // Histograms
        {
            let histos = self
                .histograms
                .lock()
                .expect("metrics histograms lock should not be poisoned");
            for (name, family) in histos.iter() {
                let _ = writeln!(out, "# HELP {name} {}", family.help);
                let _ = writeln!(out, "# TYPE {name} histogram");
                for row in &family.rows {
                    for (bound, count) in &row.buckets {
                        let mut labels = row.labels.clone();
                        labels.push(("le".into(), format_bound(*bound)));
                        write_metric_line(
                            &mut out,
                            &format!("{name}_bucket"),
                            &labels,
                            *count as f64,
                        );
                    }
                    // +Inf bucket
                    {
                        let mut labels = row.labels.clone();
                        labels.push(("le".into(), "+Inf".into()));
                        write_metric_line(
                            &mut out,
                            &format!("{name}_bucket"),
                            &labels,
                            row.count as f64,
                        );
                    }
                    write_metric_line(&mut out, &format!("{name}_sum"), &row.labels, row.sum);
                    write_metric_line(
                        &mut out,
                        &format!("{name}_count"),
                        &row.labels,
                        row.count as f64,
                    );
                }
                // If no observations, emit a single zeroed bucket set.
                if family.rows.is_empty() {
                    for bound in DURATION_BUCKETS {
                        let _ = writeln!(out, "{name}_bucket{{le=\"{bound}\"}} 0");
                    }
                    let _ = writeln!(out, "{name}_bucket{{le=\"+Inf\"}} 0");
                    let _ = writeln!(out, "{name}_sum 0");
                    let _ = writeln!(out, "{name}_count 0");
                }
            }
        }

        // Gauges
        {
            let gauges = self
                .gauges
                .lock()
                .expect("metrics gauges lock should not be poisoned");
            for (name, g) in gauges.iter() {
                let _ = writeln!(out, "# HELP {name} {}", g.help);
                let _ = writeln!(out, "# TYPE {name} gauge");
                let _ = writeln!(out, "{name} {}", g.value);
            }
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Helper: write a single metric line with optional labels
// ---------------------------------------------------------------------------

fn write_metric_line(out: &mut String, name: &str, labels: &[(String, String)], value: f64) {
    if labels.is_empty() {
        let _ = writeln!(out, "{name} {value}");
    } else {
        let label_str: String = labels
            .iter()
            .map(|(k, v)| format!("{k}=\"{v}\""))
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(out, "{name}{{{label_str}}} {value}");
    }
}

fn format_bound(bound: f64) -> String {
    if bound.fract() == 0.0 {
        format!("{bound:.0}")
    } else if bound * 1000.0 % 1.0 == 0.0 {
        format!("{bound:.3}")
    } else {
        bound.to_string()
    }
}

// ---------------------------------------------------------------------------
// Axum handler for GET /metrics
// ---------------------------------------------------------------------------

pub async fn metrics_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        registry().render(),
    )
}

/// Record bounded request dimensions after the downstream response exists.
pub async fn request_metrics_middleware(request: Request<Body>, next: Next) -> Response {
    let method = metric_method(request.method().as_str());
    let route = metric_route(request.uri().path());
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    record_response_metrics(
        registry(),
        method,
        route,
        response.status(),
        started.elapsed().as_secs_f64(),
    );
    response
}

fn record_response_metrics(
    registry: &MetricsRegistry,
    method: &'static str,
    route: &'static str,
    status: StatusCode,
    duration_seconds: f64,
) {
    let status_class = match status.as_u16() {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    };
    registry.inc_counter(
        "hydra_requests_total",
        vec![
            ("method".to_owned(), method.to_owned()),
            ("route".to_owned(), route.to_owned()),
            ("status_class".to_owned(), status_class.to_owned()),
        ],
    );
    registry.observe_histogram(
        "hydra_request_duration_seconds",
        duration_seconds.max(0.0),
        vec![
            ("method".to_owned(), method.to_owned()),
            ("route".to_owned(), route.to_owned()),
        ],
    );
}

fn metric_method(method: &str) -> &'static str {
    match method {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "PATCH" => "PATCH",
        "DELETE" => "DELETE",
        "HEAD" => "HEAD",
        "OPTIONS" => "OPTIONS",
        _ => "OTHER",
    }
}

fn metric_route(path: &str) -> &'static str {
    match path {
        "/" => "/",
        "/healthz" => "/healthz",
        "/readyz" => "/readyz",
        "/readyz/details" => "/readyz/details",
        "/metrics" => "/metrics",
        "/mcp" => "/mcp",
        "/a2a" => "/a2a",
        _ if path.starts_with("/v1/nexus/") => "/v1/nexus/*",
        _ if path.starts_with("/v1/") => "/v1/*",
        _ if path.starts_with("/static/") => "/static/*",
        _ => "/other",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_registry_basic_counters() {
        let reg = MetricsRegistry::new();
        let output = reg.render();

        // All pre-registered names must appear.
        assert!(output.contains("# HELP hydra_requests_total"));
        assert!(output.contains("# TYPE hydra_requests_total counter"));
        assert!(output.contains("# HELP hydra_envelopes_total"));
        assert!(output.contains("# HELP hydra_tk_nuke_aborts_total"));
        assert!(output.contains("# HELP hydra_bridge_sync_scheduler_operations_total"));
        assert!(output.contains("# HELP hydra_request_duration_seconds"));
        assert!(output.contains("# HELP hydra_tk_cache_hit_ratio"));
        assert!(output.contains("# TYPE hydra_tk_cache_hit_ratio gauge"));
        assert!(output.contains("# HELP hydra_db_connections"));
    }

    #[test]
    fn metrics_counter_increment() {
        let reg = MetricsRegistry::new();
        reg.inc_counter(
            "hydra_requests_total",
            vec![
                ("method".into(), "GET".into()),
                ("path".into(), "/test".into()),
                ("status".into(), "200".into()),
            ],
        );
        reg.inc_counter(
            "hydra_requests_total",
            vec![
                ("method".into(), "GET".into()),
                ("path".into(), "/test".into()),
                ("status".into(), "200".into()),
            ],
        );

        let output = reg.render();
        assert!(
            output.contains("hydra_requests_total{method=\"GET\",path=\"/test\",status=\"200\"} 2"),
            "counter value not 2:\n{output}"
        );
    }

    #[test]
    fn metrics_gauge_set() {
        let reg = MetricsRegistry::new();
        reg.set_gauge("hydra_tk_cache_hit_ratio", 0.97);
        let output = reg.render();
        assert!(
            output.contains("hydra_tk_cache_hit_ratio 0.97"),
            "gauge not 0.97:\n{output}"
        );
    }

    #[test]
    fn metrics_nuke_abort_records() {
        let reg = MetricsRegistry::new();
        reg.inc_counter("hydra_tk_nuke_aborts_total", vec![]);
        let output = reg.render();
        assert!(output.contains("hydra_tk_nuke_aborts_total"));
    }

    #[test]
    fn scheduler_metrics_use_only_bounded_outcome_labels() {
        let reg = MetricsRegistry::new();
        reg.inc_counter(
            "hydra_bridge_sync_scheduler_operations_total",
            vec![("outcome".into(), "proposal_succeeded".into())],
        );
        let output = reg.render();
        assert!(output.contains(
            "hydra_bridge_sync_scheduler_operations_total{outcome=\"proposal_succeeded\"} 1"
        ));
        assert!(!output.contains("tenant_id"));
        assert!(!output.contains("schedule_id"));
        assert!(!output.contains("adapter_id"));
    }

    #[test]
    fn metrics_request_records_duration() {
        let reg = MetricsRegistry::new();
        reg.inc_counter(
            "hydra_requests_total",
            vec![
                ("method".into(), "POST".into()),
                ("path".into(), "/api/test".into()),
                ("status".into(), "201".into()),
            ],
        );
        reg.observe_histogram("hydra_request_duration_seconds", 0.042, {
            vec![
                ("method".into(), "POST".into()),
                ("path".into(), "/api/test".into()),
            ]
        });

        let output = reg.render();
        assert!(
            output.contains("hydra_request_duration_seconds_count"),
            "histogram count missing"
        );
        assert!(
            output.contains("hydra_request_duration_seconds_sum"),
            "histogram sum missing"
        );
        assert!(output.contains("_bucket"), "histogram buckets missing");
    }

    #[test]
    fn metrics_envelope_records() {
        let reg = MetricsRegistry::new();
        reg.inc_counter(
            "hydra_envelopes_total",
            vec![("state".into(), "PendingApproval".into())],
        );
        let output = reg.render();
        assert!(output.contains("hydra_envelopes_total{state=\"PendingApproval\"}"));
    }

    #[test]
    fn request_metrics_record_bounded_dimensions() {
        let reg = MetricsRegistry::new();
        record_response_metrics(
            &reg,
            metric_method("TRACE"),
            metric_route("/v1/entities/tenant-secret/entity-secret?email=private"),
            StatusCode::NOT_FOUND,
            0.042,
        );
        let output = reg.render();
        assert!(output.contains(
            "hydra_requests_total{method=\"OTHER\",route=\"/v1/*\",status_class=\"4xx\"} 1"
        ));
        assert!(output
            .contains("hydra_request_duration_seconds_count{method=\"OTHER\",route=\"/v1/*\"} 1"));
        assert!(!output.contains("tenant-secret"));
        assert!(!output.contains("email"));
    }

    #[test]
    fn unknown_routes_and_methods_have_fixed_labels() {
        assert_eq!(metric_method("CUSTOM-IDENTITY"), "OTHER");
        assert_eq!(metric_route("/customer/secret-id"), "/other");
        assert_eq!(metric_route("/v1/nexus/context/secret-id"), "/v1/nexus/*");
    }
}
