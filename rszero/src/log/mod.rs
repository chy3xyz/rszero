//! Structured logging via tracing.
//!
//! Initializes a global tracing subscriber with configurable format, level, and output target.

use tracing_subscriber::prelude::*;
use crate::config::LogConfig;

/// Initialize the global tracing subscriber from [`LogConfig`].
///
/// Supports stdout and file output. File output includes log rotation
/// by date (daily rotation, keeping 7 days of history).
pub fn init(config: &LogConfig) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true);

    let fmt_layer = if config.format == "json" {
        fmt_layer.json().boxed()
    } else {
        fmt_layer.compact().boxed()
    };

    let subscriber = tracing_subscriber::registry()
        .with(env_filter);

    if config.output == "stdout" || config.output.is_empty() {
        subscriber.with(fmt_layer).init();
    } else {
        // File output with daily rotation
        let file_appender = tracing_appender::rolling::daily(&config.output, "rszero");
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        let file_fmt_layer = tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_target(true)
            .with_thread_ids(true);

        let file_fmt_layer = if config.format == "json" {
            file_fmt_layer.json().boxed()
        } else {
            file_fmt_layer.compact().boxed()
        };

        subscriber
            .with(fmt_layer)
            .with(file_fmt_layer)
            .init();

        // Prevent guard from being dropped
        std::mem::forget(_guard);
    }
}

/// Log at INFO level.
pub fn info(msg: &str) { tracing::info!("{}", msg) }
/// Log at WARN level.
pub fn warn(msg: &str) { tracing::warn!("{}", msg) }
/// Log at ERROR level.
pub fn error(msg: &str) { tracing::error!("{}", msg) }
/// Log at DEBUG level.
pub fn debug(msg: &str) { tracing::debug!("{}", msg) }
/// Create a named info span.
pub fn trace_span(name: &str) -> tracing::Span { tracing::info_span!("{}", name) }
