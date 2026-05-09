//! Background job worker built on the scheduler and queue systems.
//!
//! Provides a worker pool that consumes messages from a queue and executes
//! tasks with retry, timeout, and dead-letter support.
//!
//! # Example
//! ```ignore
//! use rszero::worker::{Worker, JobConfig};
//! use rszero::queue::Queue;
//!
//! let worker = Worker::new("email-worker", queue)
//!     .with_concurrency(4)
//!     .with_max_retries(3);
//! worker.run(|msg| async move {
//!     tracing::info!("processing: {}", msg.payload);
//!     Ok(())
//! }).await;
//! ```

use crate::error::RszeroResult;
use crate::queue::Message;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// Job handler type.
pub type JobHandler = Arc<dyn Fn(Message) -> Pin<Box<dyn Future<Output = RszeroResult<()>> + Send>> + Send + Sync>;

/// Job processing configuration.
#[derive(Debug, Clone)]
pub struct JobConfig {
    /// Number of concurrent workers.
    pub concurrency: usize,
    /// Maximum retry attempts.
    pub max_retries: u32,
    /// Timeout per job.
    pub timeout: Duration,
    /// Retry backoff base duration.
    pub retry_backoff: Duration,
}

impl Default for JobConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            max_retries: 3,
            timeout: Duration::from_secs(30),
            retry_backoff: Duration::from_secs(1),
        }
    }
}

/// Background worker for processing queue messages.
pub struct Worker {
    name: String,
    config: JobConfig,
    dlq_tx: Option<tokio::sync::mpsc::Sender<crate::queue::Message>>,
}

impl Worker {
    /// Create a new worker with the given name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            config: JobConfig::default(),
            dlq_tx: None,
        }
    }

    /// Set a dead-letter queue sender for failed messages.
    pub fn with_dlq(mut self, tx: tokio::sync::mpsc::Sender<crate::queue::Message>) -> Self {
        self.dlq_tx = Some(tx);
        self
    }

    /// Set concurrency level.
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.config.concurrency = n.max(1);
        self
    }

    /// Set max retries.
    pub fn with_max_retries(mut self, n: u32) -> Self {
        self.config.max_retries = n;
        self
    }

    /// Set job timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Run the worker with a handler function and a message stream.
    ///
    /// `handler` is called for each message. The worker manages concurrency,
    /// retries, and timeouts automatically.
    ///
    /// # Example
    /// ```ignore
    /// use rszero::worker::Worker;
    /// use rszero::queue::Message;
    ///
    /// let worker = Worker::new("email-worker");
    /// let rx = message_channel; // tokio::sync::mpsc::Receiver<Message>
    /// worker.run(rx, |msg| async move {
    ///     tracing::info!("processing: {}", msg.payload);
    ///     Ok(())
    /// }).await;
    /// ```
    pub async fn run<F, Fut>(
        self,
        mut rx: tokio::sync::mpsc::Receiver<Message>,
        handler: F,
    ) -> RszeroResult<()>
    where
        F: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = RszeroResult<()>> + Send + 'static,
    {
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));
        let handler = Arc::new(handler);
        let timeout = self.config.timeout;
        let max_retries = self.config.max_retries;
        let retry_backoff = self.config.retry_backoff;
        let dlq_tx = self.dlq_tx;
        let mut join_set = tokio::task::JoinSet::new();

        tracing::info!(
            worker = %self.name,
            concurrency = self.config.concurrency,
            "worker started, waiting for messages"
        );

        while let Some(msg) = rx.recv().await {
            let permit = semaphore.clone().acquire_owned().await
                .map_err(|e| crate::error::RszeroError::Internal { message: format!("semaphore error: {}", e), source: None })?;
            let handler = handler.clone();
            let dlq_tx = dlq_tx.clone();

            join_set.spawn(async move {
                let _permit = permit;
                let result = tokio::time::timeout(timeout, async {
                    execute_with_retry(
                        || handler(msg.clone()),
                        max_retries,
                        retry_backoff,
                    ).await
                }).await;

                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::error!(error = %e, "job failed after retries");
                        if let Some(ref tx) = dlq_tx {
                            let _ = tx.try_send(msg);
                        }
                    }
                    Err(_) => {
                        tracing::error!("job timed out");
                        if let Some(ref tx) = dlq_tx {
                            let _ = tx.try_send(msg);
                        }
                    }
                }
            });
        }

        // Wait for all in-flight tasks to complete before shutting down
        tracing::info!(worker = %self.name, "worker receiver closed, waiting for in-flight tasks");
        while join_set.join_next().await.is_some() {}
        tracing::info!(worker = %self.name, "worker shutting down");
        Ok(())
    }

    /// Get the worker name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the worker config.
    pub fn config(&self) -> &JobConfig {
        &self.config
    }
}

/// Execute a single job with retry logic.
pub async fn execute_with_retry<F, Fut>(
    f: F,
    max_retries: u32,
    backoff: Duration,
) -> RszeroResult<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = RszeroResult<()>>,
{
    let mut last_error = None;
    for attempt in 0..=max_retries {
        match f().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!(attempt, error = %e, "job failed, retrying");
                last_error = Some(e);
                if attempt < max_retries {
                    // Use saturating shift to avoid overflow when attempt >= 32.
                    let multiplier = if attempt >= 31 { u32::MAX } else { 1u32 << attempt };
                    let delay = backoff.saturating_mul(multiplier).min(Duration::from_secs(60));
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| crate::error::RszeroError::Internal { message: "all retries exhausted".into(), source: None }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_config() {
        let worker = Worker::new("test")
            .with_concurrency(8)
            .with_max_retries(5)
            .with_timeout(Duration::from_secs(60));
        assert_eq!(worker.config().concurrency, 8);
        assert_eq!(worker.config().max_retries, 5);
    }

    #[tokio::test]
    async fn test_execute_with_retry_success() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = count.clone();
        let result = execute_with_retry(
            move || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                }
            },
            3,
            Duration::from_millis(10),
        ).await;
        assert!(result.is_ok());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_execute_with_retry_eventual_success() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = count.clone();
        let result = execute_with_retry(
            move || {
                let c = c.clone();
                async move {
                    if c.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 2 {
                        Err(crate::error::RszeroError::Internal { message: "temp".into(), source: None })
                    } else {
                        Ok(())
                    }
                }
            },
            3,
            Duration::from_millis(10),
        ).await;
        assert!(result.is_ok());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_execute_with_retry_high_attempts_no_overflow() {
        // Regression test: attempt >= 32 should not panic on shift overflow
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = count.clone();
        let start = std::time::Instant::now();
        let result = execute_with_retry(
            move || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Err::<(), _>(crate::error::RszeroError::Internal { message: "always fail".into(), source: None })
                }
            },
            35,
            Duration::from_millis(0), // zero backoff for fast test
        ).await;
        assert!(result.is_err());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 36);
        // Should complete quickly because zero backoff
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
