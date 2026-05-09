//! Structured access log middleware.
//!
//! Records detailed HTTP request/response information in a structured format
//! compatible with nginx access logs and ELK stack ingestion.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;

/// Access log entry structure.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AccessLogEntry {
    /// Request timestamp in ISO8601.
    pub timestamp: String,
    /// HTTP method.
    pub method: String,
    /// Request URI.
    pub uri: String,
    /// Client IP address.
    pub client_ip: Option<String>,
    /// User-Agent header.
    pub user_agent: Option<String>,
    /// HTTP status code.
    pub status: u16,
    /// Response duration in milliseconds.
    pub duration_ms: u64,
    /// Request ID from correlation header.
    pub request_id: Option<String>,
    /// Response content length (if known).
    pub response_length: Option<u64>,
}

/// Access log middleware that records detailed request information.
pub async fn access_log_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers();

    let client_ip = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());

    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let request_id = headers
        .get(super::REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let start = Instant::now();
    let res = next.run(req).await;
    let elapsed = start.elapsed();
    let status = res.status().as_u16();

    let response_length = res
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    let entry = AccessLogEntry {
        timestamp: crate::utils::now_iso8601(),
        method: method.to_string(),
        uri: uri.to_string(),
        client_ip,
        user_agent,
        status,
        duration_ms: elapsed.as_millis() as u64,
        request_id,
        response_length,
    };

    // Emit as JSON for structured logging
    match serde_json::to_string(&entry) {
        Ok(json) => tracing::info!(target: "access_log", %json),
        Err(_) => tracing::info!(
            target: "access_log",
            method = %entry.method,
            uri = %entry.uri,
            status = entry.status,
            duration_ms = entry.duration_ms,
        ),
    }

    res
}

/// Tower-compatible wrapper for access log middleware.
pub struct AccessLog;

impl AccessLog {
    /// Create an axum middleware function for access logging.
    pub fn middleware() -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
        |req, next| Box::pin(access_log_middleware(req, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_log_entry_serialize() {
        let entry = AccessLogEntry {
            timestamp: "2024-01-01T00:00:00Z".into(),
            method: "GET".into(),
            uri: "/health".into(),
            client_ip: Some("127.0.0.1".into()),
            user_agent: Some("curl/8.0".into()),
            status: 200,
            duration_ms: 12,
            request_id: Some("req-123".into()),
            response_length: Some(42),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("GET"));
        assert!(json.contains("/health"));
        assert!(json.contains("200"));
    }
}
