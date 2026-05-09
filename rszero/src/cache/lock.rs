//! Redis distributed lock using SET NX EX with Lua scripts for safe release.
//!
//! Implements the Redlock algorithm subset with atomic release via Lua script,
//! ensuring a lock is only released by the client that acquired it.

use crate::cache::Cache;
use crate::error::{RszeroError, RszeroResult};
use std::time::Duration;
use fred::interfaces::{KeysInterface, LuaInterface};
use fred::prelude::RedisValue;

/// Lua script for safe lock release.
/// Only deletes the key if the value matches (owner verification).
const RELEASE_SCRIPT: &str = r#"
    if redis.call("get", KEYS[1]) == ARGV[1] then
        return redis.call("del", KEYS[1])
    else
        return 0
    end
"#;

/// Distributed lock acquired from Redis.
pub struct DistributedLock {
    cache: Cache,
    resource: String,
    value: String,
}

impl Drop for DistributedLock {
    fn drop(&mut self) {
        // Best-effort release via Lua script in a blocking runtime call.
        // In production, always call `release().await` explicitly.
        let resource = std::mem::take(&mut self.resource);
        let value = std::mem::take(&mut self.value);
        let cache = self.cache.clone();
        let key = format!("rszero:lock:{}", resource);
        tokio::spawn(async move {
            let _: Result<fred::prelude::RedisValue, _> = cache.client().eval(
                r#"
                    if redis.call("get", KEYS[1]) == ARGV[1] then
                        return redis.call("del", KEYS[1])
                    else
                        return 0
                    end
                "#,
                vec![key],
                vec![value],
            ).await;
        });
        tracing::debug!(resource, "distributed lock dropped (best-effort release spawned)");
    }
}

impl DistributedLock {
    /// Try to acquire a distributed lock with the given TTL.
    ///
    /// Uses `SET key value NX EX ttl` for atomic acquisition.
    /// Returns `Some(lock)` if acquired, `None` if already held.
    pub async fn try_acquire(cache: Cache, resource: &str, ttl: Duration) -> RszeroResult<Option<Self>> {
        let value = format!("{}-{}", uuid::Uuid::new_v4(), std::process::id());
        let key = format!("rszero:lock:{}", resource);

        // Use SET NX EX for atomic acquire
        // fred 6.0 SET with NX and EX options
        let result = cache.client().set(
            &key,
            &value,
            Some(fred::types::Expiration::EX(ttl.as_secs() as i64)),
            Some(fred::types::SetOptions::NX),
            false,
        ).await;

        match result {
            Ok(RedisValue::Integer(1)) | Ok(RedisValue::String(_)) => {
                tracing::info!(resource, "distributed lock acquired");
                Ok(Some(Self {
                    cache,
                    resource: resource.to_string(),
                    value,
                }))
            }
            Ok(_) => Ok(None), // NX failed, key already exists
            Err(e) => Err(RszeroError::Cache { message: format!("lock acquire failed: {}", e), source: None }),
        }
    }

    /// Release the lock using a Lua script for atomic owner verification.
    ///
    /// Returns `Ok(true)` if the lock was released, `Ok(false)` if the lock
    /// had already expired or was acquired by another client.
    pub async fn release(&self) -> RszeroResult<bool> {
        let key = format!("rszero:lock:{}", self.resource);

        let result = self.cache.client().eval(
            RELEASE_SCRIPT,
            vec![key],
            vec![self.value.clone()],
        ).await;

        match result {
            Ok(RedisValue::Integer(1)) => {
                tracing::debug!(resource = %self.resource, "distributed lock released");
                Ok(true)
            }
            Ok(_) => {
                tracing::warn!(
                    resource = %self.resource,
                    "lock release failed - lock may have expired or been acquired by another client"
                );
                Ok(false)
            }
            Err(e) => Err(RszeroError::Cache { message: format!("lock release failed: {}", e), source: None }),
        }
    }

    /// Refresh the lock TTL (extend the lease).
    pub async fn refresh(&self, ttl: Duration) -> RszeroResult<bool> {
        let key = format!("rszero:lock:{}", self.resource);

        let result = self.cache.client().eval(
            r#"
                if redis.call("get", KEYS[1]) == ARGV[1] then
                    return redis.call("expire", KEYS[1], ARGV[2])
                else
                    return 0
                end
            "#,
            vec![key],
            vec![self.value.clone(), ttl.as_secs().to_string()],
        ).await;

        match result {
            Ok(RedisValue::Integer(1)) => Ok(true),
            Ok(_) => Ok(false),
            Err(e) => Err(RszeroError::Cache { message: format!("lock refresh failed: {}", e), source: None }),
        }
    }

    /// Get the lock resource name.
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// Get the unique lock value (owner token).
    pub fn token(&self) -> &str {
        &self.value
    }
}

/// Acquire a lock, run a closure, and auto-release on completion.
///
/// If the closure panics, the lock is still released on drop.
pub async fn with_lock<T, F, Fut>(
    cache: Cache,
    resource: &str,
    ttl: Duration,
    f: F,
) -> RszeroResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let lock = DistributedLock::try_acquire(cache, resource, ttl)
        .await?
        .ok_or_else(|| RszeroError::Cache { message: format!("failed to acquire lock: {}", resource), source: None })?;

    let result = f().await;
    let released = lock.release().await?;
    if !released {
        tracing::warn!(resource, "lock was not released (may have expired)");
    }
    // Explicitly drop to avoid the best-effort auto-release warning
    drop(lock);
    Ok(result)
}

/// Acquire a lock with automatic refresh in a background task.
///
/// The lock is refreshed every `refresh_interval` until the closure completes.
pub async fn with_lock_auto_refresh<T, F, Fut>(
    cache: Cache,
    resource: &str,
    ttl: Duration,
    refresh_interval: Duration,
    f: F,
) -> RszeroResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let lock = DistributedLock::try_acquire(cache.clone(), resource, ttl)
        .await?
        .ok_or_else(|| RszeroError::Cache { message: format!("failed to acquire lock: {}", resource), source: None })?;

    let lock_resource = lock.resource().to_string();
    let lock_token = lock.token().to_string();

    // Spawn refresh task
    let cache_for_refresh = cache.clone();
    let refresh_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(refresh_interval);
        loop {
            interval.tick().await;
            let key = format!("rszero:lock:{}", lock_resource);
            let result = cache_for_refresh.client().eval(
                r#"
                    if redis.call("get", KEYS[1]) == ARGV[1] then
                        return redis.call("expire", KEYS[1], ARGV[2])
                    else
                        return 0
                    end
                "#,
                vec![key],
                vec![lock_token.clone(), ttl.as_secs().to_string()],
            ).await;
            if let Ok(RedisValue::Integer(0)) | Err(_) = result {
                break;
            }
        }
    });

    let result = f().await;

    refresh_handle.abort();
    let _ = refresh_handle.await;

    // Release the lock
    lock.release().await?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_struct() {
        let _lock: Option<DistributedLock> = None;
    }

    #[test]
    fn test_release_script() {
        assert!(RELEASE_SCRIPT.contains("redis.call"));
        assert!(RELEASE_SCRIPT.contains("del"));
    }
}
