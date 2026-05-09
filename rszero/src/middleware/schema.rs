//! JSON Schema validation middleware.
//!
//! Performs lightweight structural validation on request bodies
//! using serde_json::Value without external schema DSL.
//!
//! # Usage
//! ```ignore
//! use rszero::middleware::schema::{RequestSchema, FieldRule, schema_validation_middleware};
//! use std::sync::Arc;
//!
//! let schema = Arc::new(RequestSchema::new()
//!     .field("name", vec![FieldRule::Required, FieldRule::String])
//!     .field("age", vec![FieldRule::Required, FieldRule::Number, FieldRule::Range { min: 0.0, max: 150.0 }]));
//!
//! app.layer(axum::middleware::from_fn_with_state(schema, schema_validation_middleware));
//! ```

use axum::extract::Request;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use serde_json::Value;
use crate::error::ErrorResponse;

/// Validation rule for a single field.
#[derive(Debug, Clone)]
pub enum FieldRule {
    /// Field must be present.
    Required,
    /// Field must be a string.
    String,
    /// Field must be a number.
    Number,
    /// Field must be a boolean.
    Bool,
    /// Field must be an array.
    Array,
    /// Field must be an object.
    Object,
    /// String must match regex pattern (simplified: contains).
    Pattern(String),
    /// Number must be in range [min, max].
    Range {
        /// Minimum value (inclusive).
        min: f64,
        /// Maximum value (inclusive).
        max: f64,
    },
    /// String must not exceed max length.
    MaxLen(usize),
}

/// Schema definition for request body validation.
#[derive(Debug, Clone, Default)]
pub struct RequestSchema {
    /// Top-level field rules.
    pub fields: Vec<(String, Vec<FieldRule>)>,
}

impl RequestSchema {
    /// Create an empty schema.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add field rules.
    pub fn field(mut self, name: &str, rules: Vec<FieldRule>) -> Self {
        self.fields.push((name.to_string(), rules));
        self
    }

    /// Validate a JSON value against this schema.
    pub fn validate(&self, value: &Value) -> Result<(), String> {
        let obj = match value {
            Value::Object(o) => o,
            _ => return Err("request body must be a JSON object".into()),
        };

        for (field_name, rules) in &self.fields {
            let field_val = obj.get(field_name);

            for rule in rules {
                match rule {
                    FieldRule::Required => {
                        if field_val.is_none() || field_val == Some(&Value::Null) {
                            return Err(format!("field '{}' is required", field_name));
                        }
                    }
                    FieldRule::String => {
                        if let Some(v) = field_val {
                            if !v.is_string() {
                                return Err(format!("field '{}' must be a string", field_name));
                            }
                        }
                    }
                    FieldRule::Number => {
                        if let Some(v) = field_val {
                            if !v.is_number() {
                                return Err(format!("field '{}' must be a number", field_name));
                            }
                        }
                    }
                    FieldRule::Bool => {
                        if let Some(v) = field_val {
                            if !v.is_boolean() {
                                return Err(format!("field '{}' must be a boolean", field_name));
                            }
                        }
                    }
                    FieldRule::Array => {
                        if let Some(v) = field_val {
                            if !v.is_array() {
                                return Err(format!("field '{}' must be an array", field_name));
                            }
                        }
                    }
                    FieldRule::Object => {
                        if let Some(v) = field_val {
                            if !v.is_object() {
                                return Err(format!("field '{}' must be an object", field_name));
                            }
                        }
                    }
                    FieldRule::Pattern(pat) => {
                        if let Some(Value::String(s)) = field_val {
                            if !s.contains(pat) {
                                return Err(format!("field '{}' does not match pattern", field_name));
                            }
                        }
                    }
                    FieldRule::Range { min, max } => {
                        if let Some(v) = field_val {
                            let n = v.as_f64().ok_or_else(|| format!("field '{}' must be a number", field_name))?;
                            if n < *min || n > *max {
                                return Err(format!("field '{}' out of range [{}, {}]", field_name, min, max));
                            }
                        }
                    }
                    FieldRule::MaxLen(len) => {
                        if let Some(Value::String(s)) = field_val {
                            if s.chars().count() > *len {
                                return Err(format!("field '{}' exceeds max length {}", field_name, len));
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Schema validation middleware.
///
/// Validates JSON request bodies for POST/PUT/PATCH requests against
/// the provided [`RequestSchema`]. Returns 400 Bad Request on validation failure.
///
/// Use with Axum's `from_fn_with_state`:
/// ```ignore
/// let schema = Arc::new(RequestSchema::new().field("name", vec![FieldRule::Required]));
/// app.layer(axum::middleware::from_fn_with_state(schema, schema_validation_middleware));
/// ```
pub async fn schema_validation_middleware(
    State(schema): State<std::sync::Arc<RequestSchema>>,
    req: Request,
    next: Next,
) -> Response {
    // Only validate requests that typically have bodies
    match req.method() {
        &axum::http::Method::POST
        | &axum::http::Method::PUT
        | &axum::http::Method::PATCH => {
            let (parts, body) = req.into_parts();

            let bytes = match axum::body::to_bytes(body, usize::MAX).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to read request body for validation");
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(axum::body::Body::from(
                            serde_json::to_string(&ErrorResponse::new(400, "failed to read request body")).unwrap_or_else(|_| r#"{"code":400,"msg":"failed to read request body"}"#.into()),
                        ))
                        .unwrap_or_else(|_| Response::default());
                }
            };

            // Attempt to parse and validate JSON
            if let Ok(json) = serde_json::from_slice::<Value>(&bytes) {
                if let Err(err) = schema.validate(&json) {
                    tracing::debug!(error = %err, "schema validation failed");
                    let body = serde_json::to_string(&ErrorResponse::new(400, err))
                        .unwrap_or_else(|_| r#"{"code":400,"msg":"validation failed"}"#.into());
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header(axum::http::header::CONTENT_TYPE, "application/json")
                        .body(axum::body::Body::from(body))
                        .unwrap_or_else(|_| Response::default());
                }
            }

            // Reconstruct request and continue
            let req = Request::from_parts(parts, axum::body::Body::from(bytes));
            next.run(req).await
        }
        _ => next.run(req).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::Json;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    async fn echo_handler(Json(body): axum::extract::Json<Value>) -> impl axum::response::IntoResponse {
        axum::Json(body)
    }

    #[tokio::test]
    async fn test_schema_validation_rejects_invalid() {
        let schema = std::sync::Arc::new(
            RequestSchema::new()
                .field("name", vec![FieldRule::Required, FieldRule::String])
                .field("age", vec![FieldRule::Required, FieldRule::Number, FieldRule::Range { min: 0.0, max: 150.0 }]),
        );

        let app = Router::new()
            .route("/test", post(echo_handler))
            .layer(axum::middleware::from_fn_with_state(
                schema,
                schema_validation_middleware,
            ));

        let req = HttpRequest::builder()
            .method(axum::http::Method::POST)
            .uri("/test")
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"age": 200}"#))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_schema_validation_accepts_valid() {
        let schema = std::sync::Arc::new(
            RequestSchema::new()
                .field("name", vec![FieldRule::Required, FieldRule::String])
                .field("age", vec![FieldRule::Required, FieldRule::Number, FieldRule::Range { min: 0.0, max: 150.0 }]),
        );

        let app = Router::new()
            .route("/test", post(echo_handler))
            .layer(axum::middleware::from_fn_with_state(
                schema,
                schema_validation_middleware,
            ));

        let req = HttpRequest::builder()
            .method(axum::http::Method::POST)
            .uri("/test")
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name": "alice", "age": 25}"#))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_schema_validation_skips_get() {
        let schema = std::sync::Arc::new(
            RequestSchema::new()
                .field("name", vec![FieldRule::Required]),
        );

        let app = Router::new()
            .route("/test", axum::routing::get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                schema,
                schema_validation_middleware,
            ));

        let req = HttpRequest::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn test_schema_required() {
        let schema = RequestSchema::new()
            .field("name", vec![FieldRule::Required, FieldRule::String]);

        let valid = serde_json::json!({"name": "test"});
        assert!(schema.validate(&valid).is_ok());

        let missing = serde_json::json!({});
        assert!(schema.validate(&missing).is_err());
    }

    #[test]
    fn test_schema_number_range() {
        let schema = RequestSchema::new()
            .field("age", vec![FieldRule::Number, FieldRule::Range { min: 0.0, max: 150.0 }]);

        let valid = serde_json::json!({"age": 25});
        assert!(schema.validate(&valid).is_ok());

        let invalid = serde_json::json!({"age": 200});
        assert!(schema.validate(&invalid).is_err());
    }

    #[test]
    fn test_schema_max_len() {
        let schema = RequestSchema::new()
            .field("title", vec![FieldRule::String, FieldRule::MaxLen(10)]);

        let valid = serde_json::json!({"title": "short"});
        assert!(schema.validate(&valid).is_ok());

        let invalid = serde_json::json!({"title": "this is way too long"});
        assert!(schema.validate(&invalid).is_err());
    }
}
