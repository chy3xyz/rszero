//! Single-flight pattern — prevents cache stampede / thundering herd.
//!
//! Ensures that for a given key, only one concurrent call executes the
//! expensive operation; other waiters receive the same result.
//!
//! # Example
//!
//! ```no_run
//! use rszero::concurrent::singleflight::Group;
//! use rszero::error::{RszeroResult, RszeroError};
//!
//! # async fn example() -> RszeroResult<()> {
//! let g = Group::new();
//! let result = g.work("key", || async {
//!     // expensive operation
//!     Ok::<_, RszeroError>("result")
//! }).await?;
//! # Ok(())
//! # }
//! ```

#![allow(clippy::type_complexity, clippy::items_after_test_module)]

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Single-flight group for deduplicating concurrent work.
pub struct Group<T, E> {
    inflight: Arc<RwLock<HashMap<String, tokio::sync::broadcast::Sender<Result<T, E>>>>>,
}

impl<T, E> Group<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    /// Create a new single-flight group.
    pub fn new() -> Self {
        Self {
            inflight: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Execute `f` for `key`, ensuring only one concurrent execution.
    ///
    /// If another call for the same key is already in progress, this
    /// waits for that result instead of executing `f` again.
    pub async fn work<F, Fut>(&self, key: &str, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>> + Send,
    {
        // Fast path: check if work is already in flight
        let rx = {
            let inflight = self.inflight.read().await;
            inflight.get(key).map(|tx| tx.subscribe())
        };
        if let Some(mut rx) = rx {
            match rx.recv().await {
                Ok(result) => return result,
                Err(_) => {
                    // Sender dropped, fall through to execute ourselves
                }
            }
        }

        // Slow path: we are the one who will execute
        let (tx, _) = tokio::sync::broadcast::channel(128);
        {
            let mut inflight = self.inflight.write().await;
            // Double-check in case another task inserted while we were waiting
            if let Some(existing) = inflight.get(key) {
                let mut rx = existing.subscribe();
                if let Ok(result) = rx.recv().await {
                    drop(inflight);
                    return result;
                }
            }
            inflight.insert(key.to_string(), tx.clone());
        }

        // Execute the work
        let result = f().await;

        // Broadcast result and clean up
        let _ = tx.send(result.clone());
        self.inflight.write().await.remove(key);

        result
    }

    /// Number of keys currently in flight.
    pub async fn inflight_count(&self) -> usize {
        self.inflight.read().await.len()
    }
}

impl<T, E> Default for Group<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_singleflight_deduplicates() {
        let g = Group::new();
        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn 10 concurrent calls for the same key
        let mut handles = Vec::new();
        for _ in 0..10 {
            let g = g.clone();
            let counter = counter.clone();
            handles.push(tokio::spawn(async move {
                g.work("key", || async {
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, ()>(42)
                }).await
            }));
        }

        let results: Vec<Result<Result<i32, ()>, _>> = futures_util::future::join_all(handles).await;
        for r in results {
            assert_eq!(r.unwrap().unwrap(), 42);
        }

        // The expensive operation should only execute once
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_singleflight_different_keys() {
        let g = Group::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for i in 0..5 {
            let g = g.clone();
            let counter = counter.clone();
            let key = format!("key-{}", i);
            handles.push(tokio::spawn(async move {
                g.work(&key, || async {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, ()>(i)
                }).await
            }));
        }

        let results = futures_util::future::join_all(handles).await;
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r.as_ref().unwrap().unwrap(), i);
        }

        // Each key executes independently
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

}

impl<T, E> Clone for Group<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inflight: self.inflight.clone(),
        }
    }
}
