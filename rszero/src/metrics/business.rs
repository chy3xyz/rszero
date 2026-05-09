//! Business-level metrics helpers for custom application monitoring.
//!
//! Provides convenient wrappers around Prometheus counters, gauges, and histograms
//! with automatic label management and typed recording functions.
//!
//! # Example
//!
//! ```no_run
//! use rszero::metrics::business::BusinessMetrics;
//!
//! let bm = BusinessMetrics::new("order-service");
//! bm.counter("orders.placed", &["region"]).inc(&["us-east"]);
//! bm.gauge("inventory.level", &["sku"]).set(&["SKU-001"], 150.0);
//! bm.histogram("payment.latency", &["method"]).observe(&["credit_card"], 0.25);
//! ```

use prometheus::{CounterVec, GaugeVec, HistogramVec, HistogramOpts, opts, exponential_buckets, Registry, Encoder};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Business-level metrics collector with dynamic label support.
pub struct BusinessMetrics {
    service: String,
    registry: Registry,
    counters: Mutex<HashMap<String, CounterVec>>,
    gauges: Mutex<HashMap<String, GaugeVec>>,
    histograms: Mutex<HashMap<String, HistogramVec>>,
}

impl BusinessMetrics {
    /// Create a new business metrics collector for the given service.
    pub fn new(service_name: &str) -> Arc<Self> {
        Arc::new(Self {
            service: service_name.to_string(),
            registry: Registry::new(),
            counters: Mutex::new(HashMap::new()),
            gauges: Mutex::new(HashMap::new()),
            histograms: Mutex::new(HashMap::new()),
        })
    }

    /// Get or create a counter with the given name and label names.
    pub fn counter(&self, name: &str, label_names: &[&str]) -> CounterRef {
        let full_name = format!("rszero_biz_{}_{}", self.service, name.replace('.', "_"));
        let mut counters = self.counters.lock().unwrap_or_else(|e| e.into_inner());
        let counter = counters
            .entry(full_name.clone())
            .or_insert_with(|| {
                match CounterVec::new(
                    opts!(&full_name, format!("Counter for {}", name)),
                    label_names,
                ) {
                    Ok(c) => {
                        let _ = self.registry.register(Box::new(c.clone()));
                        c
                    }
                    Err(e) => {
                        tracing::error!(error = %e, name, "counter creation failed");
                        CounterVec::new(prometheus::Opts::new("rszero_biz_counter_fallback", "fallback"), &[])
                            .unwrap_or_else(|_| CounterVec::new(prometheus::Opts::new("_", "_"), &[]).unwrap_or_else(|_| panic!("critical: cannot create fallback counter")))
                    }
                }
            })
            .clone();
        CounterRef { counter }
    }

    /// Get or create a gauge with the given name and label names.
    pub fn gauge(&self, name: &str, label_names: &[&str]) -> GaugeRef {
        let full_name = format!("rszero_biz_{}_{}", self.service, name.replace('.', "_"));
        let mut gauges = self.gauges.lock().unwrap_or_else(|e| e.into_inner());
        let gauge = gauges
            .entry(full_name.clone())
            .or_insert_with(|| {
                match GaugeVec::new(
                    opts!(&full_name, format!("Gauge for {}", name)),
                    label_names,
                ) {
                    Ok(g) => {
                        let _ = self.registry.register(Box::new(g.clone()));
                        g
                    }
                    Err(e) => {
                        tracing::error!(error = %e, name, "gauge creation failed");
                        GaugeVec::new(prometheus::Opts::new("rszero_biz_gauge_fallback", "fallback"), &[])
                            .unwrap_or_else(|_| GaugeVec::new(prometheus::Opts::new("_", "_"), &[]).unwrap_or_else(|_| panic!("critical: cannot create fallback gauge")))
                    }
                }
            })
            .clone();
        GaugeRef { gauge }
    }

    /// Get or create a histogram with the given name and label names.
    pub fn histogram(&self, name: &str, label_names: &[&str]) -> HistogramRef {
        let full_name = format!("rszero_biz_{}_{}", self.service, name.replace('.', "_"));
        let mut histograms = self.histograms.lock().unwrap_or_else(|e| e.into_inner());
        let histogram = histograms
            .entry(full_name.clone())
            .or_insert_with(|| {
                let buckets = exponential_buckets(0.001, 2.0, 15)
                    .unwrap_or_else(|e| {
                        tracing::error!(error = %e, "invalid histogram buckets, using defaults");
                        vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
                    });
                match HistogramVec::new(
                    HistogramOpts::new(&full_name, format!("Histogram for {}", name)).buckets(buckets),
                    label_names,
                ) {
                    Ok(h) => {
                        let _ = self.registry.register(Box::new(h.clone()));
                        h
                    }
                    Err(e) => {
                        tracing::error!(error = %e, name, "histogram creation failed");
                        HistogramVec::new(HistogramOpts::new("rszero_biz_histogram_fallback", "fallback"), &[])
                            .unwrap_or_else(|_| HistogramVec::new(HistogramOpts::new("_", "_"), &[]).unwrap_or_else(|_| panic!("critical: cannot create fallback histogram")))
                    }
                }
            })
            .clone();
        HistogramRef { histogram }
    }

    /// Export all business metrics in Prometheus text format.
    ///
    /// Metrics are prefixed with the service name.
    pub fn export(&self) -> String {
        let encoder = prometheus::TextEncoder::new();
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

/// Reference to a counter metric.
pub struct CounterRef {
    counter: CounterVec,
}

impl CounterRef {
    /// Increment the counter for the given label values.
    pub fn inc(&self, label_values: &[&str]) {
        self.counter.with_label_values(label_values).inc();
    }

    /// Increment the counter by a specific amount.
    pub fn add(&self, label_values: &[&str], v: f64) {
        self.counter.with_label_values(label_values).inc_by(v);
    }
}

/// Reference to a gauge metric.
pub struct GaugeRef {
    gauge: GaugeVec,
}

impl GaugeRef {
    /// Set the gauge to a specific value.
    pub fn set(&self, label_values: &[&str], v: f64) {
        self.gauge.with_label_values(label_values).set(v);
    }

    /// Increment the gauge by 1.
    pub fn inc(&self, label_values: &[&str]) {
        self.gauge.with_label_values(label_values).inc();
    }

    /// Decrement the gauge by 1.
    pub fn dec(&self, label_values: &[&str]) {
        self.gauge.with_label_values(label_values).dec();
    }

    /// Add a value to the gauge.
    pub fn add(&self, label_values: &[&str], v: f64) {
        self.gauge.with_label_values(label_values).add(v);
    }
}

/// Reference to a histogram metric.
pub struct HistogramRef {
    histogram: HistogramVec,
}

impl HistogramRef {
    /// Observe a value.
    pub fn observe(&self, label_values: &[&str], v: f64) {
        self.histogram.with_label_values(label_values).observe(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_business_counter() {
        let bm = BusinessMetrics::new("test");
        bm.counter("orders.placed", &["region"]).inc(&["us-east"]);
        let out = bm.export();
        assert!(out.contains("rszero_biz_test_orders_placed"));
    }

    #[test]
    fn test_business_gauge() {
        let bm = BusinessMetrics::new("test");
        bm.gauge("inventory", &["sku"]).set(&["A001"], 42.0);
        let out = bm.export();
        assert!(out.contains("rszero_biz_test_inventory"));
        assert!(out.contains("42"));
    }

    #[test]
    fn test_business_histogram() {
        let bm = BusinessMetrics::new("test");
        bm.histogram("latency", &["method"]).observe(&["get"], 0.05);
        let out = bm.export();
        assert!(out.contains("rszero_biz_test_latency"));
    }

    #[test]
    fn test_counter_add() {
        let bm = BusinessMetrics::new("test");
        let c = bm.counter("events", &[]);
        c.add(&[], 5.0);
        let out = bm.export();
        assert!(out.contains("5"));
    }
}
