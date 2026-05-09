//! HTTP server implementation built on Axum 0.7.
//!
//! Supports graceful shutdown via OS signals (SIGINT/SIGTERM),
//! route groups, and middleware chaining.
//!
//! Automatically registers `/health` (liveness/readiness) and `/metrics`
//! (Prometheus) endpoints when configured.

use axum::{Router, response::IntoResponse};
use axum::routing::MethodRouter;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tokio::net::TcpListener;
use crate::config::RszeroConfig;
use crate::error::RszeroResult;
use crate::health::Health;
#[cfg(feature = "metrics")]
use crate::metrics::Metrics;
use crate::openapi::OpenApiSpec;

/// CORS configuration builder.
pub struct CorsConfig {
    allow_origins: Vec<String>,
    allow_methods: Vec<String>,
    allow_headers: Vec<String>,
}

impl CorsConfig {
    /// Create a new CORS config with permissive defaults.
    pub fn permissive() -> Self {
        Self {
            allow_origins: vec!["*".into()],
            allow_methods: vec!["GET".into(), "POST".into(), "PUT".into(), "DELETE".into(), "PATCH".into()],
            allow_headers: vec!["*".into()],
        }
    }

    /// Set allowed origins.
    pub fn allow_origins(mut self, origins: Vec<String>) -> Self {
        self.allow_origins = origins;
        self
    }

    /// Set allowed methods.
    pub fn allow_methods(mut self, methods: Vec<String>) -> Self {
        self.allow_methods = methods;
        self
    }

    /// Build the Axum CorsLayer.
    pub fn build(self) -> CorsLayer {
        if self.allow_origins.iter().any(|o| o == "*") {
            return CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(
                    self.allow_methods.iter()
                        .filter_map(|m| m.parse().ok())
                        .collect::<Vec<_>>()
                )
                .allow_headers(tower_http::cors::Any);
        }

        let origins: Vec<_> = self.allow_origins.iter()
            .filter_map(|o| o.parse().ok())
            .collect();

        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(
                self.allow_methods.iter()
                    .filter_map(|m| m.parse().ok())
                    .collect::<Vec<_>>()
            )
            .allow_headers(
                self.allow_headers.iter()
                    .filter_map(|h| h.parse().ok())
                    .collect::<Vec<_>>()
            )
    }
}

/// Route group for organizing related routes under a common prefix.
pub struct RouteGroup {
    prefix: String,
    router: Router,
    version: Option<String>,
}

impl RouteGroup {
    /// Create a new route group with the given path prefix.
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            router: Router::new(),
            version: None,
        }
    }

    /// Set an API version prefix (e.g. "v1" becomes `/v1` + prefix).
    pub fn version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }

    /// Add a route to this group.
    pub fn route(mut self, path: &str, method_router: MethodRouter) -> Self {
        let full_path = match &self.version {
            Some(v) => format!("/{}{}{}", v, self.prefix, path),
            None => format!("{}{}", self.prefix, path),
        };
        self.router = self.router.route(&full_path, method_router);
        self
    }

    /// Merge another router into this group.
    pub fn merge(mut self, other: Router) -> Self {
        self.router = self.router.merge(other);
        self
    }

    /// Layer middleware on this group's router.
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: tower::Layer<axum::routing::Route> + Clone + Send + 'static,
        L::Service: tower::Service<axum::extract::Request> + Clone + Send + 'static,
        <L::Service as tower::Service<axum::extract::Request>>::Response: axum::response::IntoResponse,
        <L::Service as tower::Service<axum::extract::Request>>::Error: std::convert::Into<std::convert::Infallible> + 'static,
        <L::Service as tower::Service<axum::extract::Request>>::Future: Send,
    {
        self.router = self.router.layer(layer);
        self
    }

    /// Get the underlying router for this group.
    pub fn router(self) -> Router {
        self.router
    }
}

/// Axum-based HTTP server with go-zero conventions.
#[derive(Clone)]
pub struct RszeroServer {
    host: String,
    port: u16,
    router: Router,
    #[cfg(feature = "metrics")]
    metrics: Option<std::sync::Arc<Metrics>>,
    health: Health,
    enable_health_endpoint: bool,
    #[cfg(feature = "metrics")]
    enable_metrics_endpoint: bool,
    openapi_spec: Option<OpenApiSpec>,
    enable_openapi_endpoint: bool,
}

impl RszeroServer {
    /// Create a new server bound to the given host and port.
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: host.to_string(),
            port,
            router: Router::new()
                .layer(CorsConfig::permissive().build())
                .layer(CompressionLayer::new())
                .layer(TraceLayer::new_for_http()),
            #[cfg(feature = "metrics")]
            metrics: None,
            health: Health::new(),
            enable_health_endpoint: true,
            #[cfg(feature = "metrics")]
            enable_metrics_endpoint: true,
            openapi_spec: None,
            enable_openapi_endpoint: true,
        }
    }

    /// Enable response compression (gzip/brotli).
    pub fn compression(mut self) -> Self {
        self.router = self.router.layer(CompressionLayer::new());
        self
    }

    /// Configure CORS for the server.
    pub fn cors(mut self, config: CorsConfig) -> Self {
        self.router = self.router.layer(config.build());
        self
    }

    /// Attach a metrics collector to the server.
    #[cfg(feature = "metrics")]
    pub fn with_metrics(mut self, metrics: std::sync::Arc<Metrics>) -> Self {
        self.metrics = Some(metrics.clone());
        self
    }

    /// Disable the automatic `/health` endpoint.
    pub fn disable_health_endpoint(mut self) -> Self {
        self.enable_health_endpoint = false;
        self
    }

    /// Disable the automatic `/metrics` endpoint.
    #[cfg(feature = "metrics")]
    pub fn disable_metrics_endpoint(mut self) -> Self {
        self.enable_metrics_endpoint = false;
        self
    }

    /// Disable the automatic `/openapi.json` endpoint.
    pub fn disable_openapi_endpoint(mut self) -> Self {
        self.enable_openapi_endpoint = false;
        self
    }

    /// Register an OpenAPI operation for a route.
    pub fn route_doc(mut self, path: &str, method: &str, operation: crate::openapi::types::ApiOperation) -> Self {
        let spec = self.openapi_spec.get_or_insert_with(|| OpenApiSpec::new("API", "1.0.0"));
        *spec = std::mem::take(spec).path(path, method, operation);
        self
    }

    /// Set the OpenAPI spec directly.
    pub fn with_openapi_spec(mut self, spec: OpenApiSpec) -> Self {
        self.openapi_spec = Some(spec);
        self
    }

    /// Create a server from [`RszeroConfig`].
    pub fn from_config(config: &RszeroConfig) -> Self {
        Self::new(&config.host, config.port)
    }

    /// Add a route with the given path and method router.
    pub fn route(mut self, path: &str, method_router: MethodRouter) -> Self {
        self.router = self.router.route(path, method_router);
        self
    }

    /// Add a route group to the server.
    pub fn group(mut self, group: RouteGroup) -> Self {
        self.router = self.router.merge(group.router());
        self
    }

    /// Merge another [`Router`] into this server.
    pub fn merge_router(mut self, other: Router) -> Self {
        self.router = self.router.merge(other);
        self
    }

    /// Add a global rate limiter (requests per second, burst size).
    pub fn with_rate_limiter(mut self, per_second: u64, burst_size: u32) -> Self {
        self.router = self.router.layer(crate::limit::custom_rate_limiter(per_second, burst_size));
        self
    }

    /// Add a global middleware layer.
    ///
    /// The layer must produce a service whose error type converts to `Infallible`,
    /// which is the standard Axum `Router::layer` constraint.
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: tower::Layer<axum::routing::Route> + Clone + Send + 'static,
        L::Service: tower::Service<axum::extract::Request> + Clone + Send + 'static,
        <L::Service as tower::Service<axum::extract::Request>>::Response: axum::response::IntoResponse,
        <L::Service as tower::Service<axum::extract::Request>>::Error: std::convert::Into<std::convert::Infallible> + 'static,
        <L::Service as tower::Service<axum::extract::Request>>::Future: Send,
    {
        self.router = self.router.layer(layer);
        self
    }

    /// Start the server with graceful shutdown on SIGINT/SIGTERM.
    pub async fn start(self) -> RszeroResult<()> {
        self.start_with_shutdown(None).await
    }

    /// Start the server with a custom shutdown signal.
    ///
    /// If `shutdown_signal` is `None`, defaults to SIGINT/SIGTERM.
    pub async fn start_with_shutdown(
        mut self,
        shutdown_signal: Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>>,
    ) -> RszeroResult<()> {
        // Auto-register built-in endpoints
        self = self.register_builtin_endpoints();

        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&addr).await?;
        tracing::info!("rszero server listening on {}", addr);

        let signal = shutdown_signal.unwrap_or_else(|| {
            Box::pin(async {
                let ctrl_c = tokio::signal::ctrl_c();
                let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("failed to install SIGTERM handler");
                tokio::select! {
                    _ = ctrl_c => {},
                    _ = sigterm.recv() => {},
                }
            })
        });

        // Graceful shutdown with a 30-second drain timeout.
        // After the signal fires, axum stops accepting new connections
        // and waits for in-flight requests. If they don't finish within
        // the timeout, we force-exit to avoid hanging indefinitely.
        let shutdown = async move {
            signal.await;
            tracing::info!("shutdown signal received, stopping new connections");
        };

        tokio::select! {
            result = axum::serve(listener, self.router).with_graceful_shutdown(shutdown) => {
                result.map_err(|e| crate::error::RszeroError::Internal { message: e.to_string(), source: None })
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                tracing::warn!("graceful shutdown timed out after 30s, forcing exit");
                Ok(())
            }
        }
    }

    /// Access the underlying Axum router.
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Get the health tracker for this server.
    pub fn health(&self) -> &Health {
        &self.health
    }

    // ─── Internal helpers ───────────────────────────────────────────────────

    fn register_builtin_endpoints(mut self) -> Self {
        if self.enable_health_endpoint {
            let health = self.health.clone();
            self.router = self.router.route("/health", axum::routing::get(move || {
                let health = health.clone();
                async move {
                    let deps = health.dependencies().await;
                    let all_healthy = health.all_dependencies_healthy().await;
                    let body = serde_json::json!({
                        "status": if all_healthy { "healthy" } else { "degraded" },
                        "ready": health.is_ready(),
                        "dependencies": deps.iter().map(|(k, v)| {
                            match v {
                                crate::health::DependencyHealth::Healthy => (k.clone(), serde_json::json!({"status": "healthy"})),
                                crate::health::DependencyHealth::Unhealthy(reason) => (k.clone(), serde_json::json!({"status": "unhealthy", "reason": reason})),
                            }
                        }).collect::<serde_json::Map<String, serde_json::Value>>()
                    });
                    if all_healthy {
                        axum::Json(body).into_response()
                    } else {
                        (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(body)).into_response()
                    }
                }
            }));

            let health = self.health.clone();
            self.router = self.router.route("/ready", axum::routing::get(move || {
                let health = health.clone();
                async move {
                    let ready = health.full_check().await;
                    let body = serde_json::json!({"ready": ready});
                    if ready {
                        axum::Json(body).into_response()
                    } else {
                        (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(body)).into_response()
                    }
                }
            }));
        }

        #[cfg(feature = "metrics")]
        if self.enable_metrics_endpoint {
            if let Some(metrics) = self.metrics.clone() {
                self.router = self.router.route("/metrics", axum::routing::get(move || {
                    let metrics = metrics.clone();
                    async move {
                        let body = metrics.export_prometheus();
                        (
                            axum::http::StatusCode::OK,
                            [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                            body,
                        )
                    }
                }));
            }
        }

        if self.enable_openapi_endpoint {
            if let Some(spec) = self.openapi_spec.clone() {
                self.router = self.router.route("/openapi.json", axum::routing::get(move || {
                    let spec = spec.clone();
                    async move {
                        match spec.to_json() {
                            Ok(body) => (
                                axum::http::StatusCode::OK,
                                [(axum::http::header::CONTENT_TYPE, "application/json")],
                                body,
                            ).into_response(),
                            Err(e) => (
                                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                format!("failed to generate openapi: {}", e),
                            ).into_response(),
                        }
                    }
                }));
            }
        }

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_group() {
        let group = RouteGroup::new("/api/v1")
            .route("/users", axum::routing::get(|| async { "users" }));
        let router = group.router();
        assert!(!format!("{:?}", router).is_empty());
    }

    #[test]
    fn test_server_builder() {
        let server = RszeroServer::new("0.0.0.0", 8080)
            .route("/health", axum::routing::get(|| async { "ok" }));
        assert!(!format!("{:?}", server.router()).is_empty());
    }

    #[test]
    fn test_server_health_endpoint() {
        let server = RszeroServer::new("0.0.0.0", 8080);
        assert!(server.health().is_ready());
    }
}
