//! Idempotency key middleware for preventing duplicate request processing.
//!
//! Uses `Idempotency-Key` header to identify duplicate requests.
//! First request is processed normally; subsequent requests with the same key
//! return the cached response within the TTL window.
//!
//! # Example
//! ```ignore
//! use rszero::middleware::idempotency::{IdempotencyStore, MemoryIdempotencyStore};
//!
//! let store = MemoryIdempotencyStore::new(Duration::from_secs(3600));
//! ```

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Idempotency store trait.
pub trait IdempotencyStore: Send + Sync {
    /// Check if a key exists and return the cached status code if found.
    fn get(&self, key: &str) -> Option<u16>;
    /// Store a key with its response status.
    fn insert(&self, key: &str, status: u16);
}

/// In-memory idempotency store with TTL-based eviction.
pub struct MemoryIdempotencyStore {
    inner: Mutex<HashMap<String, (u16, Instant)>>,
    ttl: Duration,
}

impl MemoryIdempotencyStore {
    /// Create a new memory store with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    fn cleanup(&self) {
        let now = Instant::now();
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.retain(|_, (_, ts)| now.duration_since(*ts) < self.ttl);
    }
}

impl IdempotencyStore for MemoryIdempotencyStore {
    fn get(&self, key: &str) -> Option<u16> {
        self.cleanup();
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.get(key).map(|(status, _)| *status)
    }

    fn insert(&self, key: &str, status: u16) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.insert(key.to_string(), (status, Instant::now()));
    }
}

/// Idempotency middleware configuration.
pub struct IdempotencyConfig {
    /// Header name for the idempotency key.
    pub header: String,
    /// Store backend.
    pub store: Arc<dyn IdempotencyStore>,
}

impl IdempotencyConfig {
    /// Create a new config with the default header name.
    pub fn new(store: Arc<dyn IdempotencyStore>) -> Self {
        Self {
            header: "Idempotency-Key".to_string(),
            store,
        }
    }
}

/// Idempotency middleware.
pub async fn idempotency_middleware(
    config: Arc<IdempotencyConfig>,
    req: Request,
    next: Next,
) -> Response {
    let key = req
        .headers()
        .get(&config.header)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(key) = key {
        if let Some(status) = config.store.get(&key) {
            tracing::info!(idempotency_key = %key, status, "duplicate request detected");
            return (
                axum::http::StatusCode::from_u16(status).unwrap_or(axum::http::StatusCode::OK),
                axum::Json(serde_json::json!({ "code": 0, "msg": "ok", "cached": true })),
            )
                .into_response();
        }

        let res = next.run(req).await;
        let status = res.status().as_u16();
        config.store.insert(&key, status);
        return res;
    }

    next.run(req).await
}

/// Tower-compatible wrapper.
pub struct IdempotencyLayer;

impl IdempotencyLayer {
    /// Create middleware function.
    pub fn middleware(
        config: Arc<IdempotencyConfig>,
    ) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
        move |req, next| {
            let config = config.clone();
            Box::pin(idempotency_middleware(config, req, next))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_store() {
        let store = MemoryIdempotencyStore::new(Duration::from_secs(60));
        assert!(store.get("key1").is_none());
        store.insert("key1", 200);
        assert_eq!(store.get("key1").unwrap(), 200);
    }

    #[test]
    fn test_memory_store_ttl_eviction() {
        let store = MemoryIdempotencyStore::new(Duration::from_millis(1));
        store.insert("key1", 200);
        std::thread::sleep(Duration::from_millis(10));
        assert!(store.get("key1").is_none());
    }
}
