//! In-process LRU/TTL cache using dashmap for lock-free concurrent access.

use dashmap::DashMap;
use std::time::{Duration, Instant};

struct CacheEntry<V> {
    value: V,
    expires_at: Option<Instant>,
}

impl<V: Clone> Clone for CacheEntry<V> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            expires_at: self.expires_at,
        }
    }
}

impl<V> CacheEntry<V> {
    fn new(value: V, ttl: Option<Duration>) -> Self {
        Self {
            value,
            expires_at: ttl.map(|d| Instant::now() + d),
        }
    }

    fn is_expired(&self) -> bool {
        self.expires_at.map(|t| Instant::now() > t).unwrap_or(false)
    }
}

/// Thread-safe in-memory cache with TTL support, using dashmap for concurrent access.
pub struct Cache<K, V> {
    entries: DashMap<K, CacheEntry<V>>,
    capacity: usize,
}

impl<K: Clone + std::hash::Hash + Eq, V: Clone> Clone for Cache<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            capacity: self.capacity,
        }
    }
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Cache<K, V> {
    /// Create a new cache with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: DashMap::with_capacity(capacity),
            capacity,
        }
    }

    /// Get a value by key. Returns None if not found or expired.
    pub fn get(&self, key: &K) -> Option<V> {
        // Atomically remove the entry if expired to close the race window
        // between checking expiration and removal.
        if self.entries.remove_if(key, |_, entry| entry.is_expired()).is_some() {
            return None;
        }
        self.entries.get(key).map(|e| e.value.clone())
    }

    /// Set a value with no expiration.
    pub fn set(&self, key: K, value: V) {
        self.set_with_ttl(key, value, None);
    }

    /// Set a value with TTL.
    pub fn set_with_ttl(&self, key: K, value: V, ttl: Option<Duration>) {
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            // Evict first expired entry or arbitrary entry
            let expired_key = self.entries.iter()
                .find(|e| e.is_expired())
                .map(|e| e.key().clone());
            if let Some(k) = expired_key {
                self.entries.remove(&k);
            } else {
                // Remove oldest entry
                if let Some(oldest) = self.entries.iter()
                    .filter_map(|e| e.expires_at.map(|t| (e.key().clone(), t)))
                    .min_by_key(|(_, t)| *t)
                    .map(|(k, _)| k)
                {
                    self.entries.remove(&oldest);
                }
            }
        }
        self.entries.insert(key, CacheEntry::new(value, ttl));
    }

    /// Delete a key.
    pub fn delete(&self, key: &K) -> bool {
        self.entries.remove(key).is_some()
    }

    /// Check if a key exists and is not expired.
    pub fn contains(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// Get the current number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&self) {
        self.entries.clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_get() {
        let cache: Cache<String, &str> = Cache::new(10);
        cache.set("key1".to_string(), "value1");
        assert_eq!(cache.get(&"key1".to_string()), Some("value1"));
        assert_eq!(cache.get(&"key2".to_string()), None);
    }

    #[test]
    fn test_delete() {
        let cache: Cache<String, &str> = Cache::new(10);
        cache.set("key1".to_string(), "value1");
        assert!(cache.delete(&"key1".to_string()));
        assert_eq!(cache.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_ttl_expiration() {
        let cache: Cache<String, &str> = Cache::new(10);
        cache.set_with_ttl("key1".to_string(), "value1", Some(Duration::from_millis(50)));
        assert_eq!(cache.get(&"key1".to_string()), Some("value1"));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(cache.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_capacity_eviction() {
        let cache: Cache<String, i32> = Cache::new(2);
        cache.set_with_ttl("a".to_string(), 1, Some(Duration::from_secs(60)));
        cache.set_with_ttl("b".to_string(), 2, Some(Duration::from_secs(60)));
        cache.set_with_ttl("c".to_string(), 3, Some(Duration::from_secs(60)));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_clear() {
        let cache: Cache<String, i32> = Cache::new(10);
        cache.set("a".to_string(), 1);
        cache.set("b".to_string(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let cache: Arc<Cache<String, i32>> = Arc::new(Cache::new(1000));
        let mut handles = vec![];

        for i in 0..10 {
            let cache = cache.clone();
            handles.push(thread::spawn(move || {
                for j in 0..100 {
                    let key = format!("{}-{}", i, j);
                    cache.set(key.clone(), j);
                    assert_eq!(cache.get(&key), Some(j));
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(cache.len(), 1000);
    }
}
