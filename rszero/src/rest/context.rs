//! Request context for cross-cutting data propagation.
//!
//! Provides a type-safe way to attach and retrieve values from the request context,
//! similar to Go's `context.Context`. Used for trace IDs, user IDs, request metadata, etc.
//!
//! # Example
//!
//! ```ignore
//! use rszero::rest::context::RequestContext;
//!
//! # async fn example() {
//! let ctx = RequestContext::new();
//! ctx.set("user_id", 123i64).await;
//! assert_eq!(ctx.get::<i64>("user_id").await, Some(123));
//! # }
//! ```

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Type-safe key for context values.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ContextKey {
    type_id: TypeId,
    name: String,
}

impl ContextKey {
    /// Create a new typed key.
    pub fn new<T: 'static>(name: &str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name: name.to_string(),
        }
    }
}

/// Request context for propagating values across handlers and middleware.
#[derive(Clone, Default)]
pub struct RequestContext {
    inner: Arc<RwLock<HashMap<ContextKey, Arc<dyn Any + Send + Sync>>>>,
}

impl RequestContext {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a typed value into the context.
    pub async fn set<T: Any + Send + Sync + 'static>(&self, key: &str, value: T) {
        let mut map = self.inner.write().await;
        map.insert(ContextKey::new::<T>(key), Arc::new(value));
    }

    /// Get a typed value from the context.
    pub async fn get<T: Any + Clone + Send + Sync + 'static>(&self, key: &str) -> Option<T> {
        let map = self.inner.read().await;
        map.get(&ContextKey::new::<T>(key))
            .and_then(|v| v.downcast_ref::<T>())
            .cloned()
    }

    /// Remove a value from the context.
    pub async fn remove<T: Any + Clone + Send + Sync + 'static>(&self, key: &str) -> Option<T> {
        let mut map = self.inner.write().await;
        map.remove(&ContextKey::new::<T>(key))
            .and_then(|v| v.downcast_ref::<T>().cloned())
    }

    /// Check if a key exists in the context.
    pub async fn has<T: Any + Send + Sync + 'static>(&self, key: &str) -> bool {
        let map = self.inner.read().await;
        map.contains_key(&ContextKey::new::<T>(key))
    }

    /// Get the number of entries in the context.
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    /// Check if the context is empty.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Clear all entries.
    pub async fn clear(&self) {
        self.inner.write().await.clear();
    }

    /// Create a child context that inherits all values.
    pub async fn fork(&self) -> Self {
        let map = self.inner.read().await;
        let new_map = map.clone();
        drop(map);
        Self {
            inner: Arc::new(RwLock::new(new_map)),
        }
    }
}

/// Axum extension type for RequestContext.
///
/// Usage:
/// ```no_run
/// use axum::Extension;
/// use rszero::rest::context::RequestContext;
///
/// async fn handler(Extension(ctx): Extension<RequestContext>) {
///     ctx.set("key", "value");
/// }
/// ```
#[cfg(feature = "rest")]
impl axum::response::IntoResponse for RequestContext {
    fn into_response(self) -> axum::response::Response {
        axum::Json(serde_json::json!({"context": "RequestContext"})).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_set_get() {
        let ctx = RequestContext::new();
        ctx.set("user_id", 42i64).await;
        assert_eq!(ctx.get::<i64>("user_id").await, Some(42));
    }

    #[tokio::test]
    async fn test_context_missing_key() {
        let ctx = RequestContext::new();
        assert_eq!(ctx.get::<String>("missing").await, None);
    }

    #[tokio::test]
    async fn test_context_typed_keys() {
        let ctx = RequestContext::new();
        ctx.set("id", 42i64).await;
        ctx.set("id", "string".to_string()).await;

        // Both values coexist because type IDs differ
        assert_eq!(ctx.get::<i64>("id").await, Some(42));
        assert_eq!(ctx.get::<String>("id").await, Some("string".to_string()));
    }

    #[tokio::test]
    async fn test_context_fork() {
        let ctx = RequestContext::new();
        ctx.set("key", "value".to_string()).await;

        let child = ctx.fork().await;
        child.set("key", "overridden".to_string()).await;

        assert_eq!(ctx.get::<String>("key").await, Some("value".to_string()));
        assert_eq!(child.get::<String>("key").await, Some("overridden".to_string()));
    }

    #[tokio::test]
    async fn test_context_remove() {
        let ctx = RequestContext::new();
        ctx.set("temp", 123i32).await;
        assert!(ctx.has::<i32>("temp").await);

        let removed = ctx.remove::<i32>("temp").await;
        assert_eq!(removed, Some(123));
        assert!(!ctx.has::<i32>("temp").await);
    }
}
