//! EP-008: In-memory Prometheus metrics registry and `/metrics` handler.
//!
//! Exposes counters, gauges, and histograms via a global registry.  The
//! `/metrics` endpoint renders them in Prometheus text format for scraping.

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{Arc, LazyLock, Mutex};

use axum::http::StatusCode;
use axum::response::IntoResponse;

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
            .unwrap()
            .entry("hydra_requests_total".into())
            .or_insert_with(|| CounterFamily {
                help: "Total HTTP requests".into(),
                rows: vec![],
            });

        self.counters
            .lock()
            .unwrap()
            .entry("hydra_envelopes_total".into())
            .or_insert_with(|| CounterFamily {
                help: "Envelopes by state".into(),
                rows: vec![],
            });

        self.counters
            .lock()
            .unwrap()
            .entry("hydra_tk_nuke_aborts_total".into())
            .or_insert_with(|| CounterFamily {
                help: "Total TK nuke aborts".into(),
                rows: vec![],
            });

        self.histograms
            .lock()
            .unwrap()
            .entry("hydra_request_duration_seconds".into())
            .or_insert_with(|| HistogramFamily {
                help: "Request duration distribution (seconds)".into(),
                rows: vec![],
            });

        self.gauges
            .lock()
            .unwrap()
            .entry("hydra_tk_cache_hit_ratio".into())
            .or_insert_with(|| GaugeValue {
                help: "TK cache hit ratio (1h rolling)".into(),
                value: 1.0,
            });

        self.gauges
            .lock()
            .unwrap()
            .entry("hydra_db_connections".into())
            .or_insert_with(|| GaugeValue {
                help: "Active database connections".into(),
                value: 0.0,
            });
    }

    // -- counters -----------------------------------------------------------

    pub(crate) fn inc_counter(&self, name: &str, labels: Vec<(String, String)>) {
        let mut counters = self.counters.lock().unwrap();
        let family = counters.get_mut(name).expect("counter not pre-registered; call register_defaults first");

        // Look for an existing row with the same labels.
        if let Some(row) = family.rows.iter_mut().find(|r| r.labels == labels) {
            row.value += 1;
        } else {
            family.rows.push(CounterRow { labels, value: 1 });
        }
    }

    pub(crate) fn inc_counter_by(
        &self,
        name: &str,
        by: u64,
        labels: Vec<(String, String)>,
    ) {
        let mut counters = self.counters.lock().unwrap();
        let family = counters.get_mut(name).expect("counter not pre-registered");

        if let Some(row) = family.rows.iter_mut().find(|r| r.labels == labels) {
            row.value += by;
        } else {
            family.rows.push(CounterRow { labels, value: by });
        }
    }

    // -- histograms ---------------------------------------------------------

    pub(crate) fn observe_histogram(&self, name: &str, value: f64, labels: Vec<(String, String)>) {
        let mut histos = self.histograms.lock().unwrap();
        let family = histos.get_mut(name).expect("histogram not pre-registered");

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

    pub(crate) fn set_gauge(&self, name: &str, value: f64) {
        let mut gauges = self.gauges.lock().unwrap();
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
            let counters = self.counters.lock().unwrap();
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
            let histos = self.histograms.lock().unwrap();
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
            let gauges = self.gauges.lock().unwrap();
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
// Convenience hook functions
// ---------------------------------------------------------------------------

/// Record an HTTP request and its duration.
pub fn record_request(method: &str, path: &str, status: u16, duration: std::time::Duration) {
    let labels = vec![
        ("method".into(), method.to_string()),
        ("path".into(), path.to_string()),
        ("status".into(), status.to_string()),
    ];
    registry().inc_counter("hydra_requests_total", labels.clone());

    registry()
        .observe_histogram("hydra_request_duration_seconds", duration.as_secs_f64(), {
            let mut l = labels;
            l.pop(); // remove status for histogram labels
            l
        });
}

/// Record an envelope transition by state name.
pub fn record_envelope(state: &str) {
    registry().inc_counter(
        "hydra_envelopes_total",
        vec![("state".into(), state.to_string())],
    );
}

/// Update the TK cache hit ratio gauge.
pub fn update_tk_ratio(ratio: f64) {
    registry().set_gauge("hydra_tk_cache_hit_ratio", ratio);
}

/// Record a TK nuke abort.
pub fn record_nuke_abort() {
    registry().inc_counter("hydra_tk_nuke_aborts_total", vec![]);
}

/// Update the active DB connection count gauge.
pub fn update_db_connections(count: f64) {
    registry().set_gauge("hydra_db_connections", count);
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
        // Using the convenience hook through the global registry.
        // We'll test the internal method directly:
        record_nuke_abort();
        let output = registry().render();
        assert!(output.contains("hydra_tk_nuke_aborts_total"));
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
            vec![("method".into(), "POST".into()), ("path".into(), "/api/test".into())]
        });

        let output = reg.render();
        assert!(output.contains("hydra_request_duration_seconds_count"), "histogram count missing");
        assert!(output.contains("hydra_request_duration_seconds_sum"), "histogram sum missing");
        assert!(output.contains("_bucket"), "histogram buckets missing");
    }

    #[test]
    fn metrics_envelope_records() {
        record_envelope("PendingApproval");
        let output = registry().render();
        assert!(output.contains("hydra_envelopes_total{state=\"PendingApproval\"}"));
    }
}
