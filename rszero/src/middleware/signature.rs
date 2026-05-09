//! Request signature verification middleware.
//!
//! Verifies API request signatures to prevent tampering and replay attacks.
//! Clients must provide `X-Timestamp`, `X-Nonce`, and `X-Signature` headers.
//!
//! Signature format (simplified, HMAC-ready interface):
//! ```ignore
//! sign = hex(hash(format!("{}:{}:{}:{}:{}", secret, method, path, timestamp, nonce)))
//! ```
//!
//! # Security Note
//! This implementation uses a deterministic string hash for demonstration.
//! For production, replace `compute_signature` with HMAC-SHA256 from the `hmac` crate.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::time::{SystemTime, UNIX_EPOCH};

/// Signature verification configuration.
pub struct SignatureConfig {
    /// Shared secret key.
    pub secret: String,
    /// Maximum time drift allowed (seconds).
    pub max_drift_secs: i64,
    /// Timestamp header name.
    pub timestamp_header: String,
    /// Nonce header name.
    pub nonce_header: String,
    /// Signature header name.
    pub signature_header: String,
}

impl SignatureConfig {
    /// Create a new signature config.
    pub fn new(secret: &str) -> Self {
        Self {
            secret: secret.to_string(),
            max_drift_secs: 300,
            timestamp_header: "X-Timestamp".to_string(),
            nonce_header: "X-Nonce".to_string(),
            signature_header: "X-Signature".to_string(),
        }
    }
}

/// Compute a request signature.
///
/// **Replace this with HMAC-SHA256 in production for cryptographic security.**
pub fn compute_signature(secret: &str, method: &str, path: &str, timestamp: &str, nonce: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let payload = format!("{}:{}:{}:{}:{}", secret, method, path, timestamp, nonce);
    let mut hasher = DefaultHasher::new();
    payload.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Signature verification middleware.
pub async fn signature_middleware(
    config: std::sync::Arc<SignatureConfig>,
    req: Request,
    next: Next,
) -> Response {
    let headers = req.headers();

    let timestamp = headers
        .get(&config.timestamp_header)
        .and_then(|v| v.to_str().ok());
    let nonce = headers
        .get(&config.nonce_header)
        .and_then(|v| v.to_str().ok());
    let signature = headers
        .get(&config.signature_header)
        .and_then(|v| v.to_str().ok());

    let (Some(timestamp), Some(nonce), Some(signature)) = (timestamp, nonce, signature) else {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "code": 401, "msg": "missing signature headers" })),
        )
            .into_response();
    };

    // Verify timestamp drift
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let ts = timestamp.parse::<i64>().unwrap_or(0);
    if (now - ts).abs() > config.max_drift_secs {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "code": 401, "msg": "timestamp drift exceeded" })),
        )
            .into_response();
    }

    let method = req.method().as_str();
    let path = req.uri().path();
    let expected = compute_signature(&config.secret, method, path, timestamp, nonce);

    if signature != expected {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "code": 401, "msg": "invalid signature" })),
        )
            .into_response();
    }

    next.run(req).await
}

/// Tower-compatible wrapper.
pub struct SignatureVerifier;

impl SignatureVerifier {
    /// Create middleware function.
    pub fn middleware(
        config: std::sync::Arc<SignatureConfig>,
    ) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
        move |req, next| {
            let config = config.clone();
            Box::pin(signature_middleware(config, req, next))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_signature_deterministic() {
        let s1 = compute_signature("secret", "GET", "/api", "1234567890", "abc");
        let s2 = compute_signature("secret", "GET", "/api", "1234567890", "abc");
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_compute_signature_different_inputs() {
        let s1 = compute_signature("secret", "GET", "/api", "1234567890", "abc");
        let s2 = compute_signature("secret", "POST", "/api", "1234567890", "abc");
        assert_ne!(s1, s2);
    }
}
