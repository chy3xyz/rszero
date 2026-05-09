//! HTTP response caching middleware.
//!
//! Caches successful GET responses in Redis with configurable TTL.
//! Uses the request URI as the cache key.

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

/// Cache configuration.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Cache TTL in seconds.
    pub ttl: u64,
    /// Only cache these status codes.
    pub cache_status: Vec<u16>,
    /// Only cache GET requests.
    pub cache_get_only: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            ttl: 60,
            cache_status: vec![200],
            cache_get_only: true,
        }
    }
}

/// A cached HTTP response entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedEntry {
    /// HTTP status code.
    pub status: u16,
    /// Response body bytes (base64-encoded for safety).
    pub body_b64: String,
}

impl CachedEntry {
    /// Create from status and body bytes.
    pub fn new(status: u16, body: &[u8]) -> Self {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        Self {
            status,
            body_b64: STANDARD.encode(body),
        }
    }

    /// Decode body bytes.
    pub fn body(&self) -> Vec<u8> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        STANDARD.decode(&self.body_b64).unwrap_or_default()
    }
}

/// Helper for building cache keys.
pub struct ResponseCache;

impl ResponseCache {
    /// Create a cache key from a request.
    pub fn cache_key(req: &Request) -> String {
        format!("rszero:cache:{}:{}", req.method(), req.uri())
    }
}

// ─── CacheMiddleware (requires cache feature) ───────────────────────────────

#[cfg(feature = "cache")]
/// Axum middleware that caches responses in Redis.
#[derive(Clone)]
pub struct CacheMiddleware {
    cache: crate::cache::Cache,
    config: CacheConfig,
}

#[cfg(feature = "cache")]
impl CacheMiddleware {
    /// Create a new cache middleware with the given Redis cache and config.
    pub fn new(cache: crate::cache::Cache, config: CacheConfig) -> Self {
        Self { cache, config }
    }

    /// Handle a request, returning a cached response if available.
    pub async fn handle(&self, req: Request, next: Next) -> Response {
        if !self.is_cacheable_request(&req) {
            return next.run(req).await;
        }

        let key = ResponseCache::cache_key(&req);

        // Try cache hit
        match self.cache.get::<CachedEntry>(&key).await {
            Ok(Some(entry)) => {
                tracing::debug!(key, status = entry.status, "cache hit");
                return Response::builder()
                    .status(StatusCode::from_u16(entry.status).unwrap_or(StatusCode::OK))
                    .header("X-Cache", "HIT")
                    .body(axum::body::Body::from(entry.body()))
                    .unwrap_or_else(|_| Response::default());
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = %e, "cache read failed, continuing without cache");
            }
        }

        // Execute request
        let response = next.run(req).await;

        // Try to cache the response
        if self.is_cacheable_response(&response) {
            let status = response.status().as_u16();
            let (parts, body) = response.into_parts();
            let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap_or_default();

            let entry = CachedEntry::new(status, &bytes);
            if let Err(e) = self.cache.set_ex(&key, &entry, self.config.ttl).await {
                tracing::warn!(error = %e, "cache write failed");
            }

            return Response::from_parts(parts, axum::body::Body::from(bytes));
        }

        response
    }

    fn is_cacheable_request(&self, req: &Request) -> bool {
        if self.config.cache_get_only && req.method() != axum::http::Method::GET {
            return false;
        }
        true
    }

    fn is_cacheable_response(&self, res: &Response) -> bool {
        let status = res.status().as_u16();
        self.config.cache_status.contains(&status)
    }
}

/// Axum middleware entry point for response caching.
///
/// When the `cache` feature is enabled, this expects a [`CacheMiddleware`]
/// to have been injected via [`axum::Extension`]:
///
/// ```ignore
/// use rszero::middleware::cache::{CacheMiddleware, CacheConfig};
/// use rszero::cache::Cache;
///
/// let cache = Cache::new(&config).await.unwrap();
/// let mw = CacheMiddleware::new(cache, CacheConfig::default());
/// app.layer(axum::Extension(mw));
/// ```
///
/// When the `cache` feature is disabled, this is a pass-through.
pub async fn cache_middleware(req: Request, next: Next) -> Response {
    #[cfg(feature = "cache")]
    {
        let maybe_mw = req.extensions().get::<CacheMiddleware>().cloned();
        if let Some(mw) = maybe_mw {
            return mw.handle(req, next).await;
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_default() {
        let cfg = CacheConfig::default();
        assert_eq!(cfg.ttl, 60);
        assert!(cfg.cache_get_only);
    }

    #[test]
    fn test_cache_key() {
        let req = axum::extract::Request::builder()
            .uri("/users/123")
            .body(axum::body::Body::empty())
            .unwrap();
        let key = ResponseCache::cache_key(&req);
        assert!(key.contains("/users/123"));
    }

    #[test]
    fn test_cached_entry_roundtrip() {
        let entry = CachedEntry::new(200, b"hello world");
        assert_eq!(entry.status, 200);
        assert_eq!(entry.body(), b"hello world");
    }
}
