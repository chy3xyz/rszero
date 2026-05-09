//! API Mock server — auto-generate mock responses from route definitions.
//!
//! Useful for frontend development, contract testing, and integration testing
//! without needing real backends.
//!
//! # Example
//!
//! ```no_run
//! use rszero::rest::mock::{MockServer, MockRoute, MockConfig};
//! use serde_json::json;
//!
//! # async fn example() {
//! let mut mock = MockServer::new(MockConfig::default());
//! mock.register(MockRoute {
//!     path: "/users/:id".into(),
//!     method: "GET".into(),
//!     response_body: json!({"id": 1, "name": "Alice"}),
//!     status_code: 200,
//! });
//! let router = mock.router();
//! # }
//! ```

use axum::{Router, routing::{get, post, put, delete, patch}, response::Json, http::StatusCode};
use serde_json::{json, Value};

/// A single mock route definition.
#[derive(Debug, Clone)]
pub struct MockRoute {
    /// URL path (use Axum `:param` syntax).
    pub path: String,
    /// HTTP method: GET, POST, PUT, DELETE, PATCH.
    pub method: String,
    /// JSON response body.
    pub response_body: Value,
    /// HTTP status code.
    pub status_code: u16,
}

/// Configuration for mock response behavior.
#[derive(Debug, Clone)]
pub struct MockConfig {
    /// Whether to include mock metadata headers.
    pub include_meta_headers: bool,
    /// Optional delay before responding (for simulating latency).
    pub delay_ms: u64,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            include_meta_headers: true,
            delay_ms: 0,
        }
    }
}

/// Mock server that generates responses from registered route definitions.
pub struct MockServer {
    routes: Vec<MockRoute>,
    config: MockConfig,
}

impl MockServer {
    /// Create a new mock server.
    pub fn new(config: MockConfig) -> Self {
        Self { routes: Vec::new(), config }
    }

    /// Register a mock route.
    pub fn register(&mut self, route: MockRoute) {
        self.routes.push(route);
    }

    /// Register multiple routes at once.
    pub fn register_all(&mut self, routes: Vec<MockRoute>) {
        self.routes.extend(routes);
    }

    /// Generate an Axum Router with mock handlers for all registered routes.
    pub fn router(&self) -> Router {
        let mut router = Router::new();
        for route in &self.routes {
            let status = StatusCode::from_u16(route.status_code).unwrap_or(StatusCode::OK);
            let body = route.response_body.clone();
            let include_meta = self.config.include_meta_headers;
            let delay_ms = self.config.delay_ms;

            let handler = move || async move {
                if delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                let mut response = (status, Json(body)).into_response();
                if include_meta {
                    let headers = response.headers_mut();
                    headers.insert("X-Mock-Server", "rszero".parse().expect("static header value is valid"));
                }
                response
            };

            router = match route.method.as_str() {
                "GET" => router.route(&route.path, get(handler)),
                "POST" => router.route(&route.path, post(handler)),
                "PUT" => router.route(&route.path, put(handler)),
                "DELETE" => router.route(&route.path, delete(handler)),
                "PATCH" => router.route(&route.path, patch(handler)),
                _ => router.route(&route.path, axum::routing::any(handler)),
            };
        }
        router
    }

    /// Generate a default mock JSON object based on a schema hint.
    pub fn generate_mock(hint: &str) -> Value {
        match hint {
            "uuid" => Value::String("550e8400-e29b-41d4-a716-446655440000".into()),
            "datetime" => Value::String(chrono::Utc::now().to_rfc3339()),
            "email" => Value::String("mock@example.com".into()),
            "uri" => Value::String("https://example.com".into()),
            "int" => Value::Number(42.into()),
            "float" => Value::Number(serde_json::Number::from_f64(std::f64::consts::PI).expect("PI is a valid JSON number")),
            "bool" => Value::Bool(true),
            _ => Value::String("mock".into()),
        }
    }

    /// Build a collection of standard REST mock routes for a resource.
    pub fn resource_routes(resource: &str, example: Value) -> Vec<MockRoute> {
        let list_path = format!("/{}", resource);
        let detail_path = format!("/{}/:id", resource);
        vec![
            MockRoute {
                path: list_path.clone(),
                method: "GET".into(),
                response_body: Value::Array(vec![example.clone()]),
                status_code: 200,
            },
            MockRoute {
                path: list_path,
                method: "POST".into(),
                response_body: example.clone(),
                status_code: 201,
            },
            MockRoute {
                path: detail_path.clone(),
                method: "GET".into(),
                response_body: example.clone(),
                status_code: 200,
            },
            MockRoute {
                path: detail_path.clone(),
                method: "PUT".into(),
                response_body: example.clone(),
                status_code: 200,
            },
            MockRoute {
                path: detail_path,
                method: "DELETE".into(),
                response_body: json!({"deleted": true}),
                status_code: 200,
            },
        ]
    }
}

// Bring IntoResponse into scope
use axum::response::IntoResponse;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_config_default() {
        let cfg = MockConfig::default();
        assert!(cfg.include_meta_headers);
        assert_eq!(cfg.delay_ms, 0);
    }

    #[test]
    fn test_mock_route_registration() {
        let mut server = MockServer::new(MockConfig::default());
        server.register(MockRoute {
            path: "/test".into(),
            method: "GET".into(),
            response_body: json!({"ok": true}),
            status_code: 200,
        });
        assert_eq!(server.routes.len(), 1);
    }

    #[test]
    fn test_generate_mock() {
        assert_eq!(MockServer::generate_mock("int"), json!(42));
        assert_eq!(MockServer::generate_mock("bool"), json!(true));
    }

    #[test]
    fn test_resource_routes() {
        let routes = MockServer::resource_routes("users", json!({"id": 1, "name": "Alice"}));
        assert_eq!(routes.len(), 5);
        let methods: Vec<_> = routes.iter().map(|r| r.method.as_str()).collect();
        assert!(methods.contains(&"GET"));
        assert!(methods.contains(&"POST"));
        assert!(methods.contains(&"DELETE"));
    }
}
