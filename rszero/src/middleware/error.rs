//! Global error handling middleware for REST API.
//!
//! Catches all unhandled errors and converts them to a standardized
//! JSON response format compatible with go-zero.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Standardized error response body.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Error code.
    pub code: i32,
    /// Error message.
    pub msg: String,
}

/// Global error handling middleware.
///
/// Wraps the response and ensures any error status is returned as JSON.
pub async fn error_middleware(req: Request, next: Next) -> Response {
    let _uri = req.uri().clone();
    let res = next.run(req).await;
    let status = res.status();

    if status.is_server_error() || status.is_client_error() {
        let code = status.as_u16() as i32;
        let msg = status.canonical_reason().unwrap_or("unknown error").to_string();
        let body = Json(ErrorBody { code, msg });
        return (status, body).into_response();
    }

    res
}

/// Tower-compatible wrapper.
pub struct GlobalErrorHandler;

impl GlobalErrorHandler {
    /// Create middleware function.
    pub fn middleware() -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
        |req, next| Box::pin(error_middleware(req, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_body_serialize() {
        let body = ErrorBody { code: 404, msg: "not found".into() };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("404"));
        assert!(json.contains("not found"));
    }
}
