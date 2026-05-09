//! Timeout enforcement for async operations.
//!
//! Replicates go-zero's per-request deadline enforcement.

#![allow(async_fn_in_trait)]

use std::future::Future;
use std::time::Duration;

/// Extension trait for adding timeouts to any future.
pub trait TimeoutExt: Future + Sized {
    /// Run the future with a timeout, returning `None` if it times out.
    async fn timeout(self, duration: Duration) -> Option<Self::Output> {
        tokio::time::timeout(duration, self).await.ok()
    }

    /// Run the future with a timeout, returning an error if it times out.
    async fn timeout_err<E>(self, duration: Duration, err: E) -> Result<Self::Output, E> {
        match tokio::time::timeout(duration, self).await {
            Ok(result) => Ok(result),
            Err(_) => Err(err),
        }
    }
}

impl<F: Future> TimeoutExt for F {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_timeout_success() {
        let result = async { 42 }.timeout(Duration::from_secs(1)).await;
        assert_eq!(result, Some(42));
    }

    #[tokio::test]
    async fn test_timeout_expires() {
        let result = async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            42
        }
        .timeout(Duration::from_millis(10))
        .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_timeout_err_success() {
        let result = async { 42 }
            .timeout_err(Duration::from_secs(1), "timeout")
            .await;
        assert_eq!(result, Ok(42));
    }

    #[tokio::test]
    async fn test_timeout_err_expires() {
        let result = async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            42
        }
        .timeout_err(Duration::from_millis(10), "timeout")
        .await;
        assert_eq!(result, Err("timeout"));
    }
}
