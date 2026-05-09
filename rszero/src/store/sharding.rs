//! Database sharding — horizontal partitioning with hash or range-based routing.
//!
//! Maps sharding keys to physical database shards, supporting automatic routing
//! for reads and writes. Complements [`ReplicaStore`] for per-shard read/write split.
//!
//! # Example
//!
//! ```no_run
//! use rszero::store::sharding::{ShardingStore, ShardStrategy};
//!
//! # async fn example() -> rszero::error::RszeroResult<()> {
//! let shards = vec![
//!     rszero::config::StoreConfig { dsn: "postgres://shard0/db".into(), max_connections: 10, min_connections: 2 },
//!     rszero::config::StoreConfig { dsn: "postgres://shard1/db".into(), max_connections: 10, min_connections: 2 },
//! ];
//! let store = ShardingStore::new(shards, ShardStrategy::Hash { slots: 256 }).await?;
//! let shard = store.route("user:12345");
//! # Ok(())
//! # }
//! ```

use crate::config::StoreConfig;
use crate::error::{RszeroError, RszeroResult};
use crate::store::Store;
use std::sync::Arc;

/// Strategy for routing keys to shards.
#[derive(Debug, Clone)]
pub enum ShardStrategy {
    /// Consistent hash with a fixed number of virtual slots.
    Hash {
        /// Number of virtual hash slots.
        slots: u32,
    },
    /// Range-based routing using a custom key extractor.
    Range {
        /// List of (start, end) ranges defining shard boundaries.
        ranges: Vec<(u64, u64)>,
    },
    /// Static modulo (key % shard_count).
    Modulo,
}

impl ShardStrategy {
    /// Determine the shard index for a given key.
    pub fn shard_index(&self, key: &str, shard_count: usize) -> usize {
        match self {
            ShardStrategy::Hash { slots } => {
                // Consistent hash: each physical shard has `slots` virtual nodes.
                // Find the virtual node whose hash is closest to the key hash
                // in clockwise direction on the hash ring.
                let key_hash = fnv1a_32(key.as_bytes()) as u64;
                let vnodes_per_shard = (*slots as usize).max(1);
                let mut best_shard = 0;
                let mut min_distance = u64::MAX;
                for shard in 0..shard_count {
                    for v in 0..vnodes_per_shard {
                        let vnode_hash = fnv1a_32(format!("{}:{}", shard, v).as_bytes()) as u64;
                        let distance = vnode_hash.wrapping_sub(key_hash);
                        if distance < min_distance {
                            min_distance = distance;
                            best_shard = shard;
                        }
                    }
                }
                best_shard
            }
            ShardStrategy::Range { ranges } => {
                let num = key.parse::<u64>().unwrap_or(0);
                for (i, (start, end)) in ranges.iter().enumerate() {
                    if num >= *start && num <= *end {
                        return i.min(shard_count - 1);
                    }
                }
                0
            }
            ShardStrategy::Modulo => {
                let hash = fnv1a_32(key.as_bytes()) as usize;
                hash % shard_count.max(1)
            }
        }
    }
}

/// A horizontally-sharded database pool.
///
/// Routes queries to the correct physical shard based on a sharding key.
#[derive(Clone)]
pub struct ShardingStore {
    shards: Arc<Vec<Store>>,
    strategy: ShardStrategy,
}

impl ShardingStore {
    /// Create a new sharded store from a list of shard configurations.
    pub async fn new(configs: Vec<StoreConfig>, strategy: ShardStrategy) -> RszeroResult<Self> {
        if configs.is_empty() {
            return Err(RszeroError::Database { message: "sharding requires at least one shard".into(), source: None });
        }
        let mut shards = Vec::with_capacity(configs.len());
        for cfg in configs {
            shards.push(Store::new(&cfg).await?);
        }
        tracing::info!(shard_count = shards.len(), strategy = ?strategy, "sharding store initialized");
        Ok(Self {
            shards: Arc::new(shards),
            strategy,
        })
    }

    /// Get the number of shards.
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    /// Route a key to its target shard and return the store reference.
    pub fn route(&self, key: &str) -> &Store {
        let idx = self.strategy.shard_index(key, self.shards.len());
        &self.shards[idx]
    }

    /// Execute an operation on the shard responsible for `key`.
    pub async fn with_shard<F, Fut, T>(&self, key: &str, f: F) -> RszeroResult<T>
    where
        F: FnOnce(&Store) -> Fut,
        Fut: std::future::Future<Output = RszeroResult<T>>,
    {
        let store = self.route(key);
        f(store).await
    }

    /// Execute an operation on all shards in parallel.
    pub async fn scatter<F, Fut, T>(&self, f: F) -> Vec<RszeroResult<T>>
    where
        F: Fn(&Store) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = RszeroResult<T>> + Send,
        T: Send + 'static,
    {
        let mut handles = Vec::new();
        for shard in self.shards.iter() {
            let f = f.clone();
            let shard = shard.clone();
            handles.push(tokio::spawn(async move { f(&shard).await }));
        }
        let mut results = Vec::with_capacity(handles.len());
        for h in handles {
            results.push(h.await.unwrap_or_else(|e| Err(RszeroError::Internal { message: format!("shard task panicked: {}", e), source: None })));
        }
        results
    }

    /// Access a specific shard by index.
    pub fn shard(&self, index: usize) -> Option<&Store> {
        self.shards.get(index)
    }

    /// Get connection pool health for all shards.
    pub async fn health_check(&self) -> Vec<(usize, crate::store::StoreHealth)> {
        let mut results = Vec::with_capacity(self.shards.len());
        for (i, shard) in self.shards.iter().enumerate() {
            results.push((i, shard.health_check().await));
        }
        results
    }

    /// Check if all shards are healthy.
    pub async fn all_healthy(&self) -> bool {
        self.health_check().await.iter().all(|(_, h)| matches!(h, crate::store::StoreHealth::Healthy))
    }
}

/// FNV-1a 32-bit hash for consistent routing.
fn fnv1a_32(data: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_strategy_routing() {
        let strategy = ShardStrategy::Hash { slots: 256 };
        let count = 4;
        // Verify keys route deterministically
        let idx1 = strategy.shard_index("user:1001", count);
        let idx2 = strategy.shard_index("user:1001", count);
        assert_eq!(idx1, idx2);
        assert!(idx1 < count);
    }

    #[test]
    fn test_modulo_strategy_routing() {
        let strategy = ShardStrategy::Modulo;
        let count = 4;
        let idx = strategy.shard_index("test_key", count);
        assert!(idx < count);
    }

    #[test]
    fn test_range_strategy_routing() {
        let strategy = ShardStrategy::Range {
            ranges: vec![(0, 1000), (1001, 2000), (2001, u64::MAX)],
        };
        let count = 3;
        assert_eq!(strategy.shard_index("500", count), 0);
        assert_eq!(strategy.shard_index("1500", count), 1);
        assert_eq!(strategy.shard_index("9999", count), 2);
    }

    #[test]
    fn test_fnv1a_deterministic() {
        let h1 = fnv1a_32(b"hello");
        let h2 = fnv1a_32(b"hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_sharding_store_empty() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            ShardingStore::new(vec![], ShardStrategy::Modulo).await
        });
        assert!(result.is_err());
    }
}
