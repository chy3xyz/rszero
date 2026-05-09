//! Request ID middleware for correlation across services.
//!
//! Generates or extracts request IDs for log correlation and distributed tracing.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::http::HeaderValue;

/// Header key for request ID.
pub const REQUEST_ID_HEADER: &str = "X-Request-Id";

/// Middleware that generates or propagates request IDs.
pub async fn request_id_middleware(req: Request, next: Next) -> Response {
    let request_id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .cloned()
        .unwrap_or_else(|| {
            HeaderValue::try_from(format!("req-{}", uuid::Uuid::new_v4().simple())).unwrap_or_else(|_| HeaderValue::from_static("req-unknown"))
        });

    let mut req = req;
    req.headers_mut().insert(REQUEST_ID_HEADER, request_id.clone());

    let mut res = next.run(req).await;
    res.headers_mut().insert(REQUEST_ID_HEADER, request_id);

    res
}

/// Extract the request ID from a response.
pub fn get_request_id(res: &Response) -> Option<&str> {
    res.headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{StatusCode, Request as HttpRequest};
    use axum::Router;
    use axum::routing::get;
    use axum::response::IntoResponse;
    use tower::ServiceExt;

    async fn echo_request_id(req: Request) -> impl IntoResponse {
        let request_id = req.headers().get(REQUEST_ID_HEADER)
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("")
            .to_string();
        axum::Json(serde_json::json!({"request_id": request_id}))
    }

    #[tokio::test]
    async fn test_request_id_generates_if_missing() {
        let app = Router::new()
            .route("/test", get(echo_request_id))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let req = HttpRequest::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let request_id = res.headers().get(REQUEST_ID_HEADER);
        assert!(request_id.is_some());
        let id_str = request_id.unwrap().to_str().unwrap();
        assert!(id_str.starts_with("req-"));
        assert_eq!(id_str.len(), 36);
    }

    #[tokio::test]
    async fn test_request_id_propagates_existing() {
        let app = Router::new()
            .route("/test", get(echo_request_id))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let req = HttpRequest::builder()
            .uri("/test")
            .header(REQUEST_ID_HEADER, "my-custom-id")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["request_id"], "my-custom-id");
    }

    #[test]
    fn test_get_request_id() {
        let mut res = Response::new(Body::empty());
        res.headers_mut().insert(REQUEST_ID_HEADER, HeaderValue::from_static("test-id"));
        assert_eq!(get_request_id(&res), Some("test-id"));
    }
}
