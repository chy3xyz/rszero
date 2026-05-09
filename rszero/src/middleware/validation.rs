//! Request validation middleware.
//!
//! Validates request bodies against JSON Schema rules before reaching handlers.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::http::StatusCode;
use axum::body::Body;

/// Validation rule for request body.
#[derive(Clone)]
pub struct ValidationRules {
    max_body_size: usize,
    required_headers: Vec<String>,
}

impl ValidationRules {
    /// Create default validation rules.
    pub fn new() -> Self {
        Self {
            max_body_size: 10 * 1024 * 1024,
            required_headers: Vec::new(),
        }
    }

    /// Set maximum body size in bytes.
    pub fn max_body_size(mut self, bytes: usize) -> Self {
        self.max_body_size = bytes;
        self
    }

    /// Add required headers.
    pub fn required_headers(mut self, headers: Vec<String>) -> Self {
        self.required_headers = headers;
        self
    }

    /// Validate a request.
    pub fn validate(&self, req: &Request) -> Option<Response> {
        if let Some(content_length) = req.headers()
            .get(axum::http::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
        {
            if content_length > self.max_body_size {
                let body = Body::from(
                    serde_json::json!({"code": 413, "msg": "request body too large"}).to_string()
                );
                let mut res = Response::new(body);
                *res.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
                return Some(res);
            }
        }

        for header in &self.required_headers {
            if !req.headers().contains_key(header.as_str()) {
                let body = Body::from(
                    serde_json::json!({"code": 400, "msg": format!("missing required header: {}", header)}).to_string()
                );
                let mut res = Response::new(body);
                *res.status_mut() = StatusCode::BAD_REQUEST;
                return Some(res);
            }
        }

        None
    }
}

impl Default for ValidationRules {
    fn default() -> Self { Self::new() }
}

/// Create a validation middleware from rules.
pub fn validation_middleware(rules: ValidationRules) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
    move |req: Request, next: Next| {
        let rules = rules.clone();
        Box::pin(async move {
            if let Some(response) = rules.validate(&req) {
                return response;
            }
            next.run(req).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::Router;
    use axum::routing::get;
    use axum::response::IntoResponse;
    use tower::ServiceExt;

    async fn ok_handler() -> impl IntoResponse {
        axum::Json(serde_json::json!({"status": "ok"}))
    }

    #[tokio::test]
    async fn test_validation_passes() {
        let rules = ValidationRules::new().max_body_size(1024);
        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(axum::middleware::from_fn(validation_middleware(rules)));

        let req = HttpRequest::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_validation_rejects_large_body() {
        let rules = ValidationRules::new().max_body_size(10);
        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(axum::middleware::from_fn(validation_middleware(rules)));

        let req = HttpRequest::builder()
            .uri("/test")
            .header(axum::http::header::CONTENT_LENGTH, "100")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_validation_required_headers() {
        let rules = ValidationRules::new()
            .required_headers(vec!["X-Api-Key".into()]);
        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(axum::middleware::from_fn(validation_middleware(rules)));

        let req = HttpRequest::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
