//! Redis cache layer built on fred 6.0.
//!
//! Also provides an in-process LRU/TTL cache via [`memcache`],
//! distributed locking via [`lock`], and cache-aside patterns via [`aside`].

pub mod memcache;
pub mod lock;
pub mod aside;
pub mod redlock;

pub use lock::{DistributedLock, with_lock};
pub use aside::{cache_aside, cache_aside_with_invalidate};

use fred::prelude::*;
use crate::config::CacheConfig;
use crate::error::{RszeroError, RszeroResult};

/// Redis cache client with serde support.
#[derive(Clone)]
pub struct Cache {
    pub(crate) client: RedisClient,
}

impl Cache {
    /// Create a new cache connection from [`CacheConfig`].
    pub async fn new(config: &CacheConfig) -> RszeroResult<Self> {
        let redis_config = RedisConfig::from_url(&format!(
            "redis://{}:{}/{}", config.host, config.port, config.db
        )).map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
        let perf = Some(PerformanceConfig::default());
        let policy = Some(ReconnectPolicy::default());
        let client = RedisClient::new(redis_config, perf, policy);
        client.connect();
        client.wait_for_connect().await
            .map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
        Ok(Self { client })
    }

    /// Get a value by key, deserializing to `T`.
    pub async fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> RszeroResult<Option<T>> {
        let val: Option<String> = self.client.get(key).await
            .map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
        match val {
            Some(s) => {
                let data: T = serde_json::from_str(&s)
                    .map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }

    /// Set a value with no expiration.
    pub async fn set(&self, key: &str, value: &impl serde::Serialize) -> RszeroResult<()> {
        let s = serde_json::to_string(value)
            .map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
        self.client.set::<(), _, _>(key, s, None, None, false).await
            .map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
        Ok(())
    }

    /// Set a value with TTL in seconds.
    pub async fn set_ex(&self, key: &str, value: &impl serde::Serialize, ttl: u64) -> RszeroResult<()> {
        let s = serde_json::to_string(value)
            .map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
        self.client.set::<(), _, _>(key, s, Some(Expiration::EX(ttl as i64)), None, false).await
            .map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
        Ok(())
    }

    /// Delete a key.
    pub async fn del(&self, key: &str) -> RszeroResult<()> {
        self.client.del::<i64, _>(key).await
            .map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
        Ok(())
    }

    /// Check if a key exists.
    pub async fn exists(&self, key: &str) -> RszeroResult<bool> {
        let count: i64 = self.client.exists(key).await
            .map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
        Ok(count > 0)
    }

    /// Ping the Redis server to check connectivity.
    pub async fn ping(&self) -> RszeroResult<()> {
        self.client.get::<Option<String>, _>("__rszero:ping__").await
            .map_err(|e| RszeroError::Cache { message: e.to_string(), source: None })?;
        Ok(())
    }

    /// Check if the cache connection is healthy.
    pub async fn is_healthy(&self) -> bool {
        self.ping().await.is_ok()
    }

    /// Start a background health probe that reports to the given [`Health`] tracker.
    pub fn monitor_health(self, health: crate::health::Health, interval: std::time::Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if self.is_healthy().await {
                    health.set_dependency("cache", crate::health::DependencyHealth::Healthy).await;
                } else {
                    health.set_dependency("cache", crate::health::DependencyHealth::Unhealthy("ping failed".into())).await;
                }
            }
        })
    }

    /// Access the underlying Redis client.
    pub fn client(&self) -> &RedisClient { &self.client }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    async fn test_cache() -> Cache {
        let config = CacheConfig {
            host: "127.0.0.1".into(),
            port: 6379,
            db: 15,
            password: None,
            pool_size: 4,
        };
        Cache::new(&config).await.expect("failed to connect to redis on db 15 — is redis-server running?")
    }

    fn test_key(name: &str) -> String {
        format!("rszero:test:cache:{}:{}", name, uuid::Uuid::new_v4())
    }

    #[tokio::test]
    async fn test_cache_set_get() {
        let cache = test_cache().await;
        let key = test_key("set_get");
        let value = serde_json::json!({"user": "alice", "id": 42});

        cache.set(&key, &value).await.unwrap();
        let retrieved: Option<serde_json::Value> = cache.get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value));

        // Cleanup
        cache.del(&key).await.unwrap();
    }

    #[tokio::test]
    async fn test_cache_set_ex_ttl() {
        let cache = test_cache().await;
        let key = test_key("ttl");
        let value = "expires-soon";

        cache.set_ex(&key, &value, 1).await.unwrap();
        let exists_before = cache.exists(&key).await.unwrap();
        assert!(exists_before);

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let exists_after = cache.exists(&key).await.unwrap();
        assert!(!exists_after);
    }

    #[tokio::test]
    async fn test_cache_del() {
        let cache = test_cache().await;
        let key = test_key("del");

        cache.set(&key, &"to-delete").await.unwrap();
        assert!(cache.exists(&key).await.unwrap());

        cache.del(&key).await.unwrap();
        assert!(!cache.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn test_cache_get_missing() {
        let cache = test_cache().await;
        let key = test_key("missing");

        let result: Option<String> = cache.get(&key).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_cache_exists() {
        let cache = test_cache().await;
        let key = test_key("exists");

        assert!(!cache.exists(&key).await.unwrap());
        cache.set(&key, &"yes").await.unwrap();
        assert!(cache.exists(&key).await.unwrap());
        cache.del(&key).await.unwrap();
    }
}
