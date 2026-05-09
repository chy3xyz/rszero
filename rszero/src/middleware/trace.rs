//! Request tracing middleware for distributed tracing.
//!
//! Extracts or generates trace IDs, creates tracing spans for each request,
//! and propagates trace context via HTTP headers.
//!
//! Supports both W3C Trace Context (`traceparent`) and legacy `X-Trace-Id` headers.
//! W3C `traceparent` takes precedence when present.
//!
//! When the `trace` feature is enabled, also integrates with OpenTelemetry
//! via the global tracer provider.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::http::HeaderValue;

/// W3C Trace Context header key.
pub const TRACEPARENT_HEADER: &str = "traceparent";
/// Legacy header key for trace ID propagation.
pub const TRACE_ID_HEADER: &str = "X-Trace-Id";
/// Legacy header key for span ID propagation.
pub const SPAN_ID_HEADER: &str = "X-Span-Id";
/// Legacy header key for parent span ID propagation.
#[allow(dead_code)]
pub const PARENT_SPAN_ID_HEADER: &str = "X-Parent-Span-Id";
/// Legacy header key for sampled flag.
pub const SAMPLED_HEADER: &str = "X-Sampled";

/// Tracing middleware that extracts or generates trace IDs and creates spans.
///
/// Priority:
/// 1. W3C `traceparent` header
/// 2. Legacy `X-Trace-Id` header
/// 3. Generate new trace ID
pub async fn trace_middleware(req: Request, next: Next) -> Response {
    // Try W3C traceparent first (only when trace feature is enabled)
    #[cfg(feature = "trace")]
    let (trace_id, span_id, sampled) = if let Some(tp) = req
        .headers()
        .get(TRACEPARENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(crate::trace::propagation::TraceContext::parse)
    {
        (tp.trace_id, tp.parent_span_id, tp.flags & 0x01 == 1)
    } else {
        fallback_trace_info(&req)
    };

    #[cfg(not(feature = "trace"))]
    let (trace_id, span_id, sampled) = fallback_trace_info(&req);

    let method = req.method().clone();
    let uri = req.uri().clone();

    // Create a tracing span for the request
    let span = tracing::info_span!(
        "http_request",
        trace_id = %trace_id,
        span_id = %span_id,
        http.method = %method,
        http.url = %uri,
        http.target = uri.path(),
    );

    let mut req = req;
    // Inject traceparent into downstream requests
    let traceparent = format!("00-{}-{}-{:02x}", trace_id, span_id, if sampled { 0x01 } else { 0x00 });
    if let Ok(v) = HeaderValue::from_str(&traceparent) {
        req.headers_mut().insert(TRACEPARENT_HEADER, v);
    }
    // Also propagate legacy headers for backward compatibility
    if let Ok(v) = HeaderValue::from_str(&trace_id) {
        req.headers_mut().insert(TRACE_ID_HEADER, v);
    }
    if let Ok(v) = HeaderValue::from_str(&span_id) {
        req.headers_mut().insert(SPAN_ID_HEADER, v);
    }

    let start = std::time::Instant::now();
    let mut res = next.run(req).await;
    let duration = start.elapsed();

    let status = res.status().as_u16();
    tracing::info!(
        parent: &span,
        http.status_code = status,
        http.duration_ms = duration.as_millis() as u64,
        "request completed"
    );

    // Return traceparent and legacy trace ID in response
    res.headers_mut().insert(TRACEPARENT_HEADER, HeaderValue::from_str(&traceparent).unwrap_or_else(|_| HeaderValue::from_static("00-00000000000000000000000000000000-0000000000000000-00")));
    res.headers_mut().insert(TRACE_ID_HEADER, HeaderValue::from_str(&trace_id).unwrap_or_else(|_| HeaderValue::from_static("unknown")));

    res
}

fn fallback_trace_info(req: &Request) -> (String, String, bool) {
    let trace_id = req
        .headers()
        .get(TRACE_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(generate_trace_id);
    let span_id = generate_span_id();
    let sampled = req.headers().get(SAMPLED_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s == "true")
        .unwrap_or(true);
    (trace_id, span_id, sampled)
}

fn generate_trace_id() -> String {
    format!("{:032x}", fastrand::u128(..))
}

fn generate_span_id() -> String {
    format!("{:016x}", fastrand::u64(..))
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

    async fn echo_trace_id(req: Request) -> impl IntoResponse {
        let trace_id = req.headers().get(TRACE_ID_HEADER)
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("")
            .to_string();
        axum::Json(serde_json::json!({"trace_id": trace_id}))
    }

    #[tokio::test]
    async fn test_trace_middleware_generates_id() {
        let app = Router::new()
            .route("/test", get(echo_trace_id))
            .layer(axum::middleware::from_fn(trace_middleware));

        let req = HttpRequest::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let trace_id = res.headers().get(TRACE_ID_HEADER);
        assert!(trace_id.is_some());
        assert_eq!(trace_id.unwrap().to_str().unwrap().len(), 32);
    }

    #[tokio::test]
    async fn test_trace_middleware_propagates_id() {
        let app = Router::new()
            .route("/test", get(echo_trace_id))
            .layer(axum::middleware::from_fn(trace_middleware));

        let req = HttpRequest::builder()
            .uri("/test")
            .header(TRACE_ID_HEADER, "custom-trace-id-1234567890")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["trace_id"], "custom-trace-id-1234567890");
    }
}
