//! Prometheus metrics integration for rszero services.
//!
//! Uses the official `prometheus` crate for counters, histograms, and gauges.
//! Provides an HTTP endpoint handler to expose metrics in Prometheus text format.
//!
//! # Example
//!
//! ```no_run
//! use rszero::metrics::Metrics;
//!
//! let metrics = Metrics::new("user-api");
//! let guard = metrics.start_request("GET", "/users");
//! // ... handle request ...
//! drop(guard); // records duration automatically
//! ```

pub mod business;

pub use business::{BusinessMetrics, CounterRef, GaugeRef, HistogramRef};

use prometheus::{CounterVec, HistogramOpts, HistogramVec, Registry, Encoder, TextEncoder, opts, exponential_buckets};
use std::time::Instant;

/// Metrics collector for a single rszero service.
pub struct Metrics {
    registry: Registry,
    requests_total: CounterVec,
    request_duration: HistogramVec,
    errors_total: CounterVec,
    active_requests: prometheus::IntGauge,
}

impl Metrics {
    /// Create a new metrics collector for the given service.
    ///
    /// # Panics
    ///
    /// Panics only if the prometheus crate itself returns an error during metric
    /// registration, which should never happen for statically-known labels.
    pub fn new(service_name: &str) -> Self {
        let registry = Registry::new();

        let requests_total = CounterVec::new(
            opts!("rszero_requests_total", "Total number of requests"),
            &["service", "method", "path", "status"],
        ).unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to create requests_total counter");
            CounterVec::new(prometheus::Opts::new("rszero_requests_total_fb", "fallback"), &[])
                .expect("fallback counter creation should not fail")
        });

        let request_duration = HistogramVec::new(
            HistogramOpts::new("rszero_request_duration_seconds", "Request duration in seconds")
                .buckets(exponential_buckets(0.001, 2.0, 15).unwrap_or_else(|e| {
                    tracing::error!(error = %e, "invalid buckets, using defaults");
                    vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
                })),
            &["service", "method", "path"],
        ).unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to create request_duration histogram");
            HistogramVec::new(HistogramOpts::new("rszero_request_duration_seconds_fb", "fallback"), &[])
                .expect("fallback histogram creation should not fail")
        });

        let errors_total = CounterVec::new(
            opts!("rszero_errors_total", "Total number of errors"),
            &["service", "error_type"],
        ).unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to create errors_total counter");
            CounterVec::new(prometheus::Opts::new("rszero_errors_total_fb", "fallback"), &[])
                .expect("fallback counter creation should not fail")
        });

        let active_requests = prometheus::IntGauge::with_opts(
            opts!("rszero_active_requests", "Current number of active requests")
                .const_label("service", service_name),
        ).unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to create active_requests gauge");
            prometheus::IntGauge::new("rszero_active_requests_fb", "fallback")
                .expect("fallback gauge creation should not fail")
        });

        let _ = registry.register(Box::new(requests_total.clone()));
        let _ = registry.register(Box::new(request_duration.clone()));
        let _ = registry.register(Box::new(errors_total.clone()));
        let _ = registry.register(Box::new(active_requests.clone()));

        Self {
            registry,
            requests_total,
            request_duration,
            errors_total,
            active_requests,
        }
    }

    /// Record the start of a request. Returns a guard that records duration on drop.
    pub fn start_request(&self, method: &str, path: &str) -> RequestGuard<'_> {
        self.active_requests.inc();
        RequestGuard {
            metrics: self,
            method: method.to_string(),
            path: path.to_string(),
            start: Instant::now(),
        }
    }

    /// Record a completed request with status code.
    pub fn record_request(&self, method: &str, path: &str, status: u16) {
        let status_label = status_class(status);
        self.requests_total
            .with_label_values(&["", method, path, status_label])
            .inc();
    }

    /// Record a failed request by error type.
    pub fn record_error(&self, error_type: &str) {
        self.errors_total
            .with_label_values(&["", error_type])
            .inc();
    }

    /// Export metrics in Prometheus text format.
    pub fn export_prometheus(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap_or_default();
        String::from_utf8(buffer).unwrap_or_default()
    }

    /// Access the underlying prometheus Registry.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Guard that records request duration and decrements active requests when dropped.
pub struct RequestGuard<'a> {
    metrics: &'a Metrics,
    method: String,
    path: String,
    start: Instant,
}

impl<'a> Drop for RequestGuard<'a> {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.metrics.active_requests.dec();
        self.metrics.request_duration
            .with_label_values(&["", &self.method, &self.path])
            .observe(duration);
    }
}

fn status_class(status: u16) -> &'static str {
    match status {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "unknown",
    }
}

/// Axum handler that exposes Prometheus metrics.
///
/// Use with Axum's `State` extractor:
/// ```ignore
/// use axum::extract::State;
/// use std::sync::Arc;
///
/// async fn handler(State(metrics): State<Arc<Metrics>>) -> String {
///     metrics.export_prometheus()
/// }
/// ```
#[cfg(feature = "rest")]
pub async fn metrics_handler(metrics: axum::extract::State<std::sync::Arc<Metrics>>) -> String {
    metrics.export_prometheus()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new("test-service");
        assert!(!metrics.export_prometheus().is_empty());
    }

    #[test]
    fn test_request_guard() {
        let metrics = Metrics::new("test");
        {
            let _guard = metrics.start_request("GET", "/test");
        }
        let output = metrics.export_prometheus();
        assert!(output.contains("rszero_request_duration_seconds"));
    }

    #[test]
    fn test_record_error() {
        let metrics = Metrics::new("test");
        metrics.record_error("database");
        let output = metrics.export_prometheus();
        assert!(output.contains("rszero_errors_total"));
    }
}
