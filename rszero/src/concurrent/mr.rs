//! MapReduce for parallel processing — replicates go-zero's `mr` module.
//!
//! # Example
//!
//! ```no_run
//! use rszero::concurrent::mr::{map_reduce, MapResult};
//!
//! #[tokio::main]
//! async fn main() {
//!     let items = vec![1, 2, 3, 4, 5];
//!     let sum: i64 = map_reduce(
//!         items,
//!         |item| Box::pin(async move { MapResult::Ok((item * 2) as i64) }),
//!         |results| results.into_iter().sum::<i64>(),
//!     ).await;
//! }
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Result of a map operation.
pub enum MapResult<T> {
    /// Successful result.
    Ok(T),
    /// Item should be discarded.
    Discard,
    /// Error occurred.
    Err(String),
}

/// Map a single item to zero or more results.
pub type Mapper<I, O> = dyn Fn(I) -> Pin<Box<dyn Future<Output = MapResult<O>> + Send>> + Send + Sync;

/// Reduce collected results into a final value.
pub type Reducer<I, O> = dyn Fn(Vec<I>) -> O + Send + Sync;

/// Parallel map-reduce over a collection of items.
///
/// Each item is processed concurrently via the mapper, then reduced.
pub async fn map_reduce<I, O, R, M, D>(
    items: Vec<I>,
    mapper: M,
    reducer: D,
) -> R
where
    I: Send + 'static,
    O: Send + 'static,
    R: Send,
    M: Fn(I) -> Pin<Box<dyn Future<Output = MapResult<O>> + Send>> + Send + Sync + Clone + 'static,
    D: Fn(Vec<O>) -> R + Send,
{
    let mapper_clone = mapper.clone();
    let handles: Vec<_> = items
        .into_iter()
        .map(move |item| {
            let m = mapper_clone.clone();
            tokio::spawn(async move {
                m(item).await
            })
        })
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(MapResult::Ok(val)) => results.push(val),
            Ok(MapResult::Discard) => {}
            Ok(MapResult::Err(e)) => tracing::warn!("map error: {}", e),
            Err(e) => tracing::error!("task panic: {}", e),
        }
    }
    reducer(results)
}

/// Map-reduce with a concurrency limit.
///
/// Processes items in batches of `concurrency` to avoid overwhelming the system.
pub async fn map_reduce_with_concurrency<I, O, R, M, D>(
    items: Vec<I>,
    mapper: M,
    reducer: D,
    concurrency: usize,
) -> R
where
    I: Send + 'static,
    O: Send + 'static,
    R: Send,
    M: Fn(I) -> Pin<Box<dyn Future<Output = MapResult<O>> + Send>> + Send + Sync + Clone + 'static,
    D: Fn(Vec<O>) -> R + Send,
{
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let handles: Vec<_> = items
        .into_iter()
        .map(|item| {
            let sem = semaphore.clone();
            let m = mapper.clone();
            tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore should not be closed while tasks are running");
                m(item).await
            })
        })
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(MapResult::Ok(val)) => results.push(val),
            Ok(MapResult::Discard) => {}
            Ok(MapResult::Err(e)) => tracing::warn!("map error: {}", e),
            Err(e) => tracing::error!("task panic: {}", e),
        }
    }
    reducer(results)
}

/// Run multiple futures concurrently and collect results.
pub async fn run_all<F, T>(futures: Vec<F>) -> Vec<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let handles: Vec<_> = futures
        .into_iter()
        .map(|f| tokio::spawn(f))
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(val) = handle.await {
            results.push(val);
        }
    }
    results
}

/// Run multiple futures, returning early on first error.
pub async fn run_all_err<F, T, E>(futures: Vec<F>) -> Result<Vec<T>, E>
where
    F: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: Send + 'static + std::fmt::Debug,
{
    let handles: Vec<_> = futures
        .into_iter()
        .map(|f| tokio::spawn(f))
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok(val)) => results.push(val),
            Ok(Err(e)) => return Err(e),
            Err(e) => panic!("run_all_err: subtask panicked: {}", e),
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_map_reduce_basic() {
        let items = vec![1, 2, 3, 4, 5];
        let sum = map_reduce(
            items,
            |item| Box::pin(async move { MapResult::Ok(item * 2) }),
            |results| results.into_iter().sum::<i32>(),
        )
        .await;
        assert_eq!(sum, 30); // (1+2+3+4+5)*2 = 30
    }

    #[tokio::test]
    async fn test_map_reduce_discard() {
        let items = vec![1, 2, 3, 4, 5];
        let count = map_reduce(
            items,
            |item| Box::pin(async move {
                if item % 2 == 0 {
                    MapResult::Ok(item)
                } else {
                    MapResult::Discard
                }
            }),
            |results| results.len(),
        )
        .await;
        assert_eq!(count, 2); // only 2 and 4
    }

    #[tokio::test]
    async fn test_run_all() {
        let futures: Vec<Pin<Box<dyn Future<Output = i32> + Send>>> = vec![
            Box::pin(async { 1 }),
            Box::pin(async { 2 }),
            Box::pin(async { 3 }),
        ];
        let results = run_all(futures).await;
        assert_eq!(results.len(), 3);
    }
}
