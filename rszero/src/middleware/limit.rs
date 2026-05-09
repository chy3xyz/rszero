//! Request body size limiting middleware.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::http::StatusCode;
use axum::body::Body;

fn too_large_response() -> Response {
    let body = Body::from(
        serde_json::json!({"code": 413, "msg": "request body too large"}).to_string()
    );
    let mut res = Response::new(body);
    *res.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
    res
}

/// Middleware that rejects requests with body exceeding the limit.
///
/// Checks `Content-Length` header first as a fast path, then enforces
/// the limit while materializing the body stream via [`axum::body::to_bytes`].
/// If the body exceeds `max_bytes`, returns 413 Payload Too Large.
///
/// The body is buffered into memory up to `max_bytes`; for file-upload
/// endpoints that may legitimately receive large bodies, use
/// `tower_http::limit::RequestBodyLimitLayer` directly instead.
pub fn body_size_limit(max_bytes: usize) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
    move |req: Request, next: Next| {
        let max = max_bytes;
        Box::pin(async move {
            // Fast path: reject immediately if Content-Length is known and too large.
            if let Some(len) = req.headers()
                .get(axum::http::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<usize>().ok())
            {
                if len > max {
                    return too_large_response();
                }
            }

            // Hard limit: materialize the body with a byte cap.
            let (parts, body) = req.into_parts();
            let bytes = match axum::body::to_bytes(body, max).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::debug!(error = %e, max, "request body exceeded size limit");
                    return too_large_response();
                }
            };

            let req = Request::from_parts(parts, Body::from(bytes));
            next.run(req).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    async fn echo_handler(body: String) -> String {
        body
    }

    #[tokio::test]
    async fn test_body_size_limit_rejects_large_body() {
        let app = Router::new()
            .route("/test", post(echo_handler))
            .layer(axum::middleware::from_fn(body_size_limit(10)));

        let req = HttpRequest::builder()
            .method(axum::http::Method::POST)
            .uri("/test")
            .header(axum::http::header::CONTENT_TYPE, "text/plain")
            .body(Body::from("this body is way too long"))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_body_size_limit_accepts_small_body() {
        let app = Router::new()
            .route("/test", post(echo_handler))
            .layer(axum::middleware::from_fn(body_size_limit(100)));

        let req = HttpRequest::builder()
            .method(axum::http::Method::POST)
            .uri("/test")
            .header(axum::http::header::CONTENT_TYPE, "text/plain")
            .body(Body::from("tiny"))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_body_size_limit_content_length_fast_path() {
        let app = Router::new()
            .route("/test", post(echo_handler))
            .layer(axum::middleware::from_fn(body_size_limit(5)));

        let req = HttpRequest::builder()
            .method(axum::http::Method::POST)
            .uri("/test")
            .header(axum::http::header::CONTENT_LENGTH, "100")
            .body(Body::from("x"))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
