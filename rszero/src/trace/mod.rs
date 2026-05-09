//! OpenTelemetry distributed tracing with Jaeger exporter.

pub mod propagation;

use opentelemetry::global;
use crate::error::RszeroResult;

/// Initialize the Jaeger tracer with the given service name.
pub fn init_tracer(service_name: &str) -> RszeroResult<()> {
    global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());
    let _tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_service_name(service_name)
        .install_simple()
        .map_err(|e| crate::error::RszeroError::Internal { message: e.to_string(), source: None })?;
    Ok(())
}

/// Shutdown the global tracer provider, flushing pending spans.
pub fn shutdown_tracer() {
    global::shutdown_tracer_provider();
}

/// Get a named tracer instance.
pub fn get_tracer(name: &'static str) -> impl opentelemetry::trace::Tracer {
    global::tracer(name)
}
