//! Canary release / gray deployment middleware.
//!
//! Routes traffic between stable and canary versions based on:
//! - Random weight (e.g. 10% canary)
//! - Header override (e.g. `X-Canary: true`)
//! - Cookie override
//!
//! The result is injected into the request context for downstream handlers.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Canary routing configuration.
pub struct CanaryConfig {
    /// Percentage of traffic to route to canary (0-100).
    pub weight: u8,
    /// Header name for forced canary routing.
    pub header_name: String,
    /// Cookie name for forced canary routing.
    pub cookie_name: String,
}

impl Default for CanaryConfig {
    fn default() -> Self {
        Self {
            weight: 0,
            header_name: "X-Canary".to_string(),
            cookie_name: "canary".to_string(),
        }
    }
}

impl CanaryConfig {
    /// Create a new config with the given weight.
    pub fn with_weight(weight: u8) -> Self {
        Self {
            weight: weight.min(100),
            ..Default::default()
        }
    }
}

/// Check if a request should be routed to canary.
pub fn is_canary(req: &Request, config: &CanaryConfig) -> bool {
    // Header override takes highest priority
    if let Some(header) = req.headers().get(&config.header_name) {
        if let Ok(val) = header.to_str() {
            return val.eq_ignore_ascii_case("true") || val == "1";
        }
    }

    // Cookie override
    if let Some(cookie) = req.headers().get(axum::http::header::COOKIE) {
        if let Ok(val) = cookie.to_str() {
            if val.contains(&format!("{}=true", config.cookie_name))
                || val.contains(&format!("{}=1", config.cookie_name))
            {
                return true;
            }
        }
    }

    // Weight-based random routing
    if config.weight > 0 {
        let roll = fastrand::u8(0..100);
        return roll < config.weight;
    }

    false
}

/// Canary middleware.
pub async fn canary_middleware(
    config: std::sync::Arc<CanaryConfig>,
    mut req: Request,
    next: Next,
) -> Response {
    let canary = is_canary(&req, &config);
    if canary {
        tracing::info!("routing to canary");
    }

    // Inject canary flag into request extensions
    req.extensions_mut().insert(CanaryFlag(canary));

    next.run(req).await
}

/// Canary flag inserted into request extensions.
#[derive(Debug, Clone, Copy)]
pub struct CanaryFlag(pub bool);

/// Tower-compatible wrapper.
pub struct CanaryLayer;

impl CanaryLayer {
    /// Create middleware function.
    pub fn middleware(
        config: std::sync::Arc<CanaryConfig>,
    ) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
        move |req, next| {
            let config = config.clone();
            Box::pin(canary_middleware(config, req, next))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canary_weight_zero() {
        let req = Request::builder().uri("/").body(axum::body::Body::empty()).unwrap();
        let config = CanaryConfig::with_weight(0);
        assert!(!is_canary(&req, &config));
    }

    #[test]
    fn test_canary_header_override() {
        let req = Request::builder()
            .uri("/")
            .header("X-Canary", "true")
            .body(axum::body::Body::empty())
            .unwrap();
        let config = CanaryConfig::with_weight(0);
        assert!(is_canary(&req, &config));
    }

    #[test]
    fn test_canary_cookie_override() {
        let req = Request::builder()
            .uri("/")
            .header("Cookie", "canary=1")
            .body(axum::body::Body::empty())
            .unwrap();
        let config = CanaryConfig::with_weight(0);
        assert!(is_canary(&req, &config));
    }
}
