//! Cache-aside pattern helper for efficient data access.

use std::future::Future;
use std::time::Duration;

/// Cache-aside pattern: try cache first, fall back to data source, then populate cache.
pub async fn cache_aside<K, V, F, Fut, E>(
    cache: &crate::cache::memcache::Cache<K, V>,
    key: K,
    ttl: Duration,
    loader: F,
) -> Result<V, E>
where
    K: std::hash::Hash + Eq + Clone + std::fmt::Debug,
    V: Clone + serde::Serialize + serde::de::DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<V, E>>,
{
    // 1. Try cache
    if let Some(value) = cache.get(&key) {
        tracing::debug!(?key, "cache hit");
        return Ok(value);
    }

    // 2. Cache miss - load from source
    tracing::debug!(?key, "cache miss");
    let value = loader().await?;

    // 3. Populate cache
    cache.set_with_ttl(key, value.clone(), Some(ttl));

    Ok(value)
}

/// Cache-aside with invalidation callback.
pub async fn cache_aside_with_invalidate<K, V, F, Fut, E, I, Ifut>(
    cache: &crate::cache::memcache::Cache<K, V>,
    key: K,
    ttl: Duration,
    loader: F,
    on_miss: I,
) -> Result<V, E>
where
    K: std::hash::Hash + Eq + Clone + std::fmt::Debug,
    V: Clone + serde::Serialize + serde::de::DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<V, E>>,
    I: FnOnce(&V) -> Ifut,
    Ifut: Future<Output = ()>,
{
    if let Some(value) = cache.get(&key) {
        return Ok(value);
    }

    let value = loader().await?;
    on_miss(&value).await;
    cache.set_with_ttl(key, value.clone(), Some(ttl));

    Ok(value)
}

/// Cache-aside with single-flight deduplication to prevent cache stampede.
///
/// When multiple concurrent callers miss the same key, only one loader
/// executes; the rest wait for the result.
pub async fn cache_aside_singleflight<K, V, F, Fut>(
    cache: &crate::cache::memcache::Cache<K, V>,
    group: &crate::concurrent::singleflight::Group<V, crate::error::RszeroError>,
    key: K,
    ttl: Duration,
    loader: F,
) -> crate::error::RszeroResult<V>
where
    K: std::hash::Hash + Eq + Clone + std::fmt::Debug + Send + Sync + 'static,
    V: Clone + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    F: FnOnce() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = crate::error::RszeroResult<V>> + Send,
{
    let cache = cache.clone();
    let key_for_work = format!("{:?}", key);
    group.work(&key_for_work, move || async move {
        if let Some(value) = cache.get(&key) {
            return Ok(value);
        }
        let value = loader().await?;
        cache.set_with_ttl(key, value.clone(), Some(ttl));
        Ok(value)
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::memcache::Cache;
    use crate::concurrent::singleflight::Group;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_cache_aside_hit() {
        let cache: Cache<String, i32> = Cache::new(10);
        cache.set("key".to_string(), 42);

        let load_count = AtomicU32::new(0);
        let result = cache_aside(
            &cache,
            "key".to_string(),
            Duration::from_secs(60),
            || async {
                load_count.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(100)
            },
        ).await.unwrap();

        assert_eq!(result, 42);
        assert_eq!(load_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_cache_aside_miss() {
        let cache: Cache<String, i32> = Cache::new(10);
        let load_count = AtomicU32::new(0);

        let result = cache_aside(
            &cache,
            "key".to_string(),
            Duration::from_secs(60),
            || async {
                load_count.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(100)
            },
        ).await.unwrap();

        assert_eq!(result, 100);
        assert_eq!(load_count.load(Ordering::SeqCst), 1);

        // Second call should hit cache
        let result2 = cache_aside(
            &cache,
            "key".to_string(),
            Duration::from_secs(60),
            || async {
                load_count.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(200)
            },
        ).await.unwrap();

        assert_eq!(result2, 100);
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_cache_aside_singleflight_dedup() {
        let cache: Cache<String, i32> = Cache::new(10);
        let group = Group::<i32, crate::error::RszeroError>::new();
        let load_count = Arc::new(AtomicU32::new(0));

        // Spawn 10 concurrent calls for the same key
        let mut handles = Vec::new();
        for _ in 0..10 {
            let cache = cache.clone();
            let group = group.clone();
            let load_count = load_count.clone();
            handles.push(tokio::spawn(async move {
                cache_aside_singleflight(
                    &cache,
                    &group,
                    "key".to_string(),
                    Duration::from_secs(60),
                    move || async move {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        load_count.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, crate::error::RszeroError>(42)
                    },
                ).await
            }));
        }

        let results = futures_util::future::join_all(handles).await;
        for r in results {
            assert_eq!(r.unwrap().unwrap(), 42);
        }

        // The loader should only execute once despite 10 concurrent misses
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
    }
}
