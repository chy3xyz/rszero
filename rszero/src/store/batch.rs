//! Batch operation helpers for sea-orm.
//!
//! Provides utilities for bulk insert, update, and delete operations.

#![allow(clippy::type_complexity)]

use crate::error::RszeroResult;

/// Batch size for chunked operations.
pub const DEFAULT_BATCH_SIZE: usize = 500;

/// Batch operation result summary.
#[derive(Debug, Clone)]
pub struct BatchResult {
    /// Number of successful operations.
    pub succeeded: usize,
    /// Number of failed operations.
    pub failed: usize,
}

impl BatchResult {
    /// Check if all operations succeeded.
    pub fn all_succeeded(&self) -> bool {
        self.failed == 0
    }
}

/// Execute a batch of database operations with error isolation.
///
/// Each operation is executed independently; failures are collected
/// without aborting the remaining batch.
pub async fn execute_batch<F, Fut>(tasks: Vec<F>) -> BatchResult
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = RszeroResult<()>>,
{
    let mut succeeded = 0;
    let mut failed = 0;

    for task in tasks {
        match task().await {
            Ok(()) => succeeded += 1,
            Err(e) => {
                tracing::warn!(error = %e, "batch operation failed");
                failed += 1;
            }
        }
    }

    BatchResult { succeeded, failed }
}

/// Chunk a large vector into smaller batches for processing.
pub fn chunk_vec<T>(items: Vec<T>, chunk_size: usize) -> Vec<Vec<T>> {
    let chunk_size = chunk_size.max(1);
    items.into_iter()
        .fold(Vec::new(), |mut acc, item| {
            if acc.last().map(|v: &Vec<T>| v.len()).unwrap_or(chunk_size + 1) >= chunk_size {
                acc.push(Vec::new());
            }
            acc.last_mut().unwrap().push(item);
            acc
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::ready;

    #[test]
    fn test_batch_result() {
        let result = BatchResult { succeeded: 10, failed: 0 };
        assert!(result.all_succeeded());

        let result = BatchResult { succeeded: 9, failed: 1 };
        assert!(!result.all_succeeded());
    }

    #[test]
    fn test_chunk_vec() {
        let items: Vec<i32> = (0..10).collect();
        let chunks = chunk_vec(items, 3);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0], vec![0, 1, 2]);
        assert_eq!(chunks[3], vec![9]);
    }

    #[test]
    fn test_chunk_vec_empty() {
        let chunks: Vec<Vec<i32>> = chunk_vec(vec![], 3);
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn test_execute_batch() {
        let tasks: Vec<Box<dyn FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = RszeroResult<()>> + Send>> + Send>> = vec![
            Box::new(|| Box::pin(ready(Ok(())))),
            Box::new(|| Box::pin(ready(Ok(())))),
            Box::new(|| Box::pin(ready(Err(crate::error::RszeroError::Internal { message: "fail".into(), source: None })))),
        ];
        let result = execute_batch(tasks).await;
        assert_eq!(result.succeeded, 2);
        assert_eq!(result.failed, 1);
    }
}
