//! Request/response logging middleware with sensitive data masking.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Default sensitive query parameter names to redact.
const SENSITIVE_QUERY_PARAMS: &[&str] = &["token", "api_key", "apikey", "password", "secret", "auth", "session", "jwt", "credential"];

/// Default sensitive header names to redact.
const SENSITIVE_HEADERS: &[&str] = &["authorization", "cookie", "x-api-key", "x-auth-token"];

/// Mask a query string by redacting sensitive parameter values.
fn mask_query(uri: &axum::http::Uri) -> String {
    let query = uri.query().unwrap_or("");
    if query.is_empty() {
        return String::new();
    }

    let masked: Vec<String> = query.split('&').map(|pair| {
        if let Some((key, _value)) = pair.split_once('=') {
            let key_lower = key.to_lowercase();
            if SENSITIVE_QUERY_PARAMS.iter().any(|s| key_lower.contains(s)) {
                format!("{}=***", key)
            } else {
                pair.to_string()
            }
        } else {
            pair.to_string()
        }
    }).collect();

    masked.join("&")
}

/// Build a safe URI string with query parameters redacted.
fn safe_uri(uri: &axum::http::Uri) -> String {
    let path = uri.path();
    let masked_query = mask_query(uri);
    if masked_query.is_empty() {
        path.to_string()
    } else {
        format!("{}?{}", path, masked_query)
    }
}

/// Log middleware that records method, URI, status, and duration.
///
/// Automatically masks sensitive query parameters and headers:
/// - Query params: `token`, `api_key`, `password`, `secret`, etc.
/// - Headers: `Authorization`, `Cookie`, `X-Api-Key`, etc.
pub async fn log_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let safe_uri_str = safe_uri(&uri);

    // Log sensitive headers at DEBUG only (and masked)
    for (name, value) in req.headers() {
        let name_lower = name.as_str().to_lowercase();
        if SENSITIVE_HEADERS.iter().any(|h| name_lower == *h) {
            let masked = value.to_str().map(|s| {
                if s.len() > 8 {
                    format!("{}...", &s[..8])
                } else {
                    "***".to_string()
                }
            }).unwrap_or_else(|_| "***".to_string());
            tracing::debug!(header = %name, value = %masked, "sensitive header present");
        }
    }

    let start = std::time::Instant::now();
    let res = next.run(req).await;
    let elapsed = start.elapsed();
    let status = res.status();
    tracing::info!(
        method = %method, uri = %safe_uri_str,
        status = status.as_u16(),
        duration_ms = elapsed.as_millis(),
        "request completed"
    );
    res
}

/// Tower-compatible layer wrapper for the log middleware.
///
/// Usage:
/// ```no_run
/// use rszero::middleware::LogMiddleware;
/// let middleware = LogMiddleware::middleware();
/// ```
pub struct LogMiddleware;

impl LogMiddleware {
    /// Create an axum middleware function for logging.
    pub fn middleware() -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
        |req, next| Box::pin(log_middleware(req, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_query_no_sensitive() {
        let uri = "/users?page=1&limit=10".parse::<axum::http::Uri>().unwrap();
        assert_eq!(mask_query(&uri), "page=1&limit=10");
    }

    #[test]
    fn test_mask_query_sensitive() {
        let uri = "/login?token=secret123&user=alice".parse::<axum::http::Uri>().unwrap();
        assert_eq!(mask_query(&uri), "token=***&user=alice");
    }

    #[test]
    fn test_mask_query_multiple_sensitive() {
        let uri = "/api?api_key=abc&password=123&name=bob".parse::<axum::http::Uri>().unwrap();
        let masked = mask_query(&uri);
        assert!(masked.contains("api_key=***"));
        assert!(masked.contains("password=***"));
        assert!(masked.contains("name=bob"));
    }

    #[test]
    fn test_safe_uri_without_query() {
        let uri = "/users".parse::<axum::http::Uri>().unwrap();
        assert_eq!(safe_uri(&uri), "/users");
    }

    #[test]
    fn test_safe_uri_with_query() {
        let uri = "/users?token=secret&page=1".parse::<axum::http::Uri>().unwrap();
        assert_eq!(safe_uri(&uri), "/users?token=***&page=1");
    }
}
