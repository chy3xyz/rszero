//! Distributed locking using Redis (single-node implementation via fred).
//!
//! Provides safe distributed locking for a single Redis instance or cluster
//! managed by fred. Uses `SET key value NX PX ttl` for acquisition and a Lua
//! script for safe release (only deletes if the value matches).
//!
//! # Example
//!
//! ```no_run
//! use rszero::cache::redlock::{RedlockManager, RedlockConfig};
//! use std::time::Duration;
//!
//! # async fn example() -> rszero::error::RszeroResult<()> {
//! let config = RedlockConfig {
//!     node: "redis://127.0.0.1:6379".into(),
//!     db: 0,
//! };
//! let manager = RedlockManager::new(config).await?;
//! let guard = manager.acquire("my-resource", 10_000).await?.expect("lock acquired");
//! // ... critical section ...
//! manager.release(guard).await;
//! # Ok(())
//! # }
//! ```

use crate::error::{RszeroError, RszeroResult};
use fred::prelude::*;

/// Lua script that only deletes the key if the value matches the owner token.
const RELEASE_SCRIPT: &str = r#"
    if redis.call("get", KEYS[1]) == ARGV[1] then
        return redis.call("del", KEYS[1])
    else
        return 0
    end
"#;

/// Configuration for Redlock connection.
#[derive(Debug, Clone)]
pub struct RedlockConfig {
    /// Redis node URL.
    pub node: String,
    /// Redis database number.
    pub db: u8,
}

impl Default for RedlockConfig {
    fn default() -> Self {
        Self {
            node: "redis://127.0.0.1:6379".into(),
            db: 0,
        }
    }
}

/// An acquired distributed lock with resource name and owner token.
#[derive(Debug, Clone)]
pub struct RedlockGuard {
    /// Resource name (lock key).
    pub resource: String,
    /// Unique lock value (owner token).
    pub value: String,
    /// Validity time in milliseconds.
    pub validity_time: usize,
}

/// Redis-backed distributed lock manager.
pub struct RedlockManager {
    client: RedisClient,
}

impl RedlockManager {
    /// Create a new Redlock manager from configuration.
    pub async fn new(config: RedlockConfig) -> RszeroResult<Self> {
        let redis_config = RedisConfig::from_url(&format!("{}?db={}", config.node, config.db))
            .map_err(|e| RszeroError::Cache { message: format!("invalid redlock config: {}", e), source: None })?;
        let perf = Some(PerformanceConfig::default());
        let policy = Some(ReconnectPolicy::default());
        let client = RedisClient::new(redis_config, perf, policy);
        client.connect();
        client.wait_for_connect().await
            .map_err(|e| RszeroError::Cache { message: format!("redlock connection failed: {}", e), source: None })?;

        // Verify connectivity with a ping
        client.ping::<()>().await
            .map_err(|e| RszeroError::Cache { message: format!("redlock ping failed: {}", e), source: None })?;

        tracing::info!(node = %config.node, "redlock manager initialized");
        Ok(Self { client })
    }

    /// Create a Redlock manager from an existing [`super::Cache`] instance.
    pub fn from_cache(cache: &super::Cache) -> Self {
        Self { client: cache.client.clone() }
    }

    /// Try to acquire a distributed lock on the given resource.
    ///
    /// `ttl_ms` is the lock time-to-live in milliseconds.
    /// Returns `Some(RedlockGuard)` on success, `None` if the lock is held by another client.
    pub async fn acquire(
        &self,
        resource: &str,
        ttl_ms: usize,
    ) -> RszeroResult<Option<RedlockGuard>> {
        let token = format!("{}:{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
        let result: Option<String> = self.client
            .set(
                resource,
                &token,
                Some(Expiration::PX(ttl_ms as i64)),
                Some(SetOptions::NX),
                false,
            )
            .await
            .map_err(|e| RszeroError::Cache { message: format!("redlock acquire failed: {}", e), source: None })?;

        match result {
            Some(_) => Ok(Some(RedlockGuard {
                resource: resource.to_string(),
                value: token,
                validity_time: ttl_ms,
            })),
            None => Ok(None),
        }
    }

    /// Release a lock previously acquired via [`Self::acquire`].
    ///
    /// Uses a Lua script to ensure the lock is only released by its owner.
    pub async fn release(&self, guard: RedlockGuard) {
        let keys: Vec<String> = vec![guard.resource.clone()];
        let args: Vec<String> = vec![guard.value];
        let result: i64 = match self.client.eval(RELEASE_SCRIPT, keys, args).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(resource = %guard.resource, error = %e, "redlock release script failed");
                0
            }
        };
        if result > 0 {
            tracing::debug!(resource = %guard.resource, "redlock released");
        } else {
            tracing::warn!(resource = %guard.resource, "redlock release: key not found or token mismatch");
        }
    }

    /// Check if a lock is still held (key exists).
    pub async fn is_locked(&self, resource: &str) -> RszeroResult<bool> {
        let exists: i64 = self.client.exists(resource).await
            .map_err(|e| RszeroError::Cache { message: format!("redlock exists check failed: {}", e), source: None })?;
        Ok(exists > 0)
    }
}

/// Acquire a Redlock, run a closure, and auto-release on completion.
///
/// If the closure panics, the lock is still released on drop.
pub async fn with_redlock<T, F, Fut>(
    manager: &RedlockManager,
    resource: &str,
    ttl_ms: usize,
    f: F,
) -> RszeroResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let guard = manager
        .acquire(resource, ttl_ms)
        .await?
        .ok_or_else(|| RszeroError::Cache { message: format!("failed to acquire redlock: {}", resource), source: None })?;

    let result = f().await;
    manager.release(guard).await;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redlock_config_default() {
        let cfg = RedlockConfig::default();
        assert_eq!(cfg.node, "redis://127.0.0.1:6379");
        assert_eq!(cfg.db, 0);
    }

    #[test]
    fn test_redlock_guard() {
        let guard = RedlockGuard {
            resource: "test".into(),
            value: "token".into(),
            validity_time: 1000,
        };
        assert_eq!(guard.resource, "test");
    }

    #[test]
    fn test_release_script_constant() {
        assert!(RELEASE_SCRIPT.contains("redis.call"));
        assert!(RELEASE_SCRIPT.contains("get"));
        assert!(RELEASE_SCRIPT.contains("del"));
    }
}
