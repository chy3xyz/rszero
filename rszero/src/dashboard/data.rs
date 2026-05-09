//! Data collection for the rszero dashboard.
//!
//! Gathers system, application, health, and metrics information
//! from various framework modules.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// Global process start time.
static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Global request counters (updated by middleware if integrated).
static TOTAL_REQUESTS: AtomicU64 = AtomicU64::new(0);
static ERROR_REQUESTS: AtomicU64 = AtomicU64::new(0);

/// Increment total request count.
pub fn record_request() {
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);
}

/// Increment error request count.
pub fn record_error() {
    ERROR_REQUESTS.fetch_add(1, Ordering::Relaxed);
}

// ─── System Info ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub rust_version: String,
    pub pid: u32,
    pub uptime_seconds: u64,
}

impl SystemInfo {
    pub fn collect() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            rust_version: option_env!("CARGO_PKG_RUST_VERSION")
                .unwrap_or("unknown")
                .to_string(),
            pid: std::process::id(),
            uptime_seconds: START_TIME.elapsed().as_secs(),
        }
    }
}

// ─── Application Info ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub total_requests: u64,
    pub error_requests: u64,
    pub error_rate: f64,
}

impl AppInfo {
    pub fn collect() -> Self {
        let total = TOTAL_REQUESTS.load(Ordering::Relaxed);
        let errors = ERROR_REQUESTS.load(Ordering::Relaxed);
        let error_rate = if total > 0 {
            (errors as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        Self {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: START_TIME.elapsed().as_secs(),
            total_requests: total,
            error_requests: errors,
            error_rate,
        }
    }
}

// ─── Health Status ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct HealthStatus {
    pub overall: bool,
    pub checks: Vec<HealthCheck>,
}

#[derive(Debug, Serialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: String,
    pub message: Option<String>,
}

impl HealthStatus {
    pub fn from_health(health: &crate::health::Health) -> Self {
        let overall = health.is_ready();
        // Note: dependency checks are async; we report readiness synchronously here.
        Self {
            overall,
            checks: vec![
                HealthCheck {
                    name: "framework".into(),
                    status: if overall { "healthy".into() } else { "unhealthy".into() },
                    message: None,
                },
            ],
        }
    }
}

// ─── Metrics Snapshot ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MetricsSnapshot {
    pub total_requests: u64,
    pub error_requests: u64,
    pub error_rate: f64,
    pub uptime_seconds: u64,
    pub uptime_formatted: String,
}

impl MetricsSnapshot {
    pub fn collect() -> Self {
        let total = TOTAL_REQUESTS.load(Ordering::Relaxed);
        let errors = ERROR_REQUESTS.load(Ordering::Relaxed);
        let uptime = START_TIME.elapsed();

        Self {
            total_requests: total,
            error_requests: errors,
            error_rate: if total > 0 {
                (errors as f64 / total as f64) * 100.0
            } else {
                0.0
            },
            uptime_seconds: uptime.as_secs(),
            uptime_formatted: format_duration(uptime),
        }
    }
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let mins = secs / 60;
    let hours = mins / 60;
    let days = hours / 24;
    if days > 0 {
        format!("{}d {:02}h {:02}m", days, hours % 24, mins % 60)
    } else if hours > 0 {
        format!("{}h {:02}m {:02}s", hours, mins % 60, secs % 60)
    } else {
        format!("{}m {:02}s", mins, secs % 60)
    }
}

// ─── Route Info ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RouteInfo {
    pub method: String,
    pub path: String,
}

/// Simple route registry for dashboard display.
/// Users can register routes here if they want them shown in the dashboard.
pub struct RouteRegistry {
    routes: std::sync::Mutex<Vec<RouteInfo>>,
}

impl RouteRegistry {
    /// Create a new empty route registry.
    pub fn new() -> Self {
        Self {
            routes: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Register a route for display in the dashboard.
    pub fn register(&self, method: &str, path: &str) {
        if let Ok(mut routes) = self.routes.lock() {
            routes.push(RouteInfo {
                method: method.to_string(),
                path: path.to_string(),
            });
        }
    }

    /// List all registered routes.
    pub fn list(&self) -> Vec<RouteInfo> {
        self.routes
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default()
    }
}

impl Default for RouteRegistry {
    fn default() -> Self {
        Self::new()
    }
}
