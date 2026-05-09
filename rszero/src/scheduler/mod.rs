//! Lightweight task scheduler for periodic and delayed jobs.
//!
//! Built on tokio::time, supports interval-based scheduling without
//! external cron parsing dependencies.
//!
//! # Example
//! ```ignore
//! use rszero::scheduler::Scheduler;
//! use std::time::Duration;
//!
//! let sched = Scheduler::new();
//! sched.every(Duration::from_secs(60), || async {
//!     tracing::info!("running periodic task");
//! }).await;
//! ```

pub mod cron;

use std::future::Future;
use std::time::Duration;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

/// Scheduled job handle.
///
/// When dropped, the scheduled task is cancelled automatically.
pub struct JobHandle {
    _handle: tokio::task::JoinHandle<()>,
    cancel: CancellationToken,
}

impl JobHandle {
    /// Cancel the scheduled job explicitly.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl Drop for JobHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Lightweight task scheduler.
pub struct Scheduler;

impl Scheduler {
    /// Create a new scheduler.
    pub fn new() -> Self {
        Self
    }

    /// Run a task at a fixed interval.
    ///
    /// The first execution happens after the interval elapses.
    /// The task is cancelled when the returned [`JobHandle`] is dropped.
    pub fn every<F, Fut>(self, period: Duration, task: F) -> JobHandle
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move {
            let mut ticker = interval(period);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        task().await;
                    }
                    _ = cancel_clone.cancelled() => break,
                }
            }
        });
        JobHandle { _handle: handle, cancel }
    }

    /// Run a task once after a delay.
    pub fn after<F, Fut>(self, delay: Duration, task: F) -> JobHandle
    where
        F: FnOnce() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(delay) => {
                    task().await;
                }
                _ = cancel_clone.cancelled() => {}
            }
        });
        JobHandle { _handle: handle, cancel }
    }

    /// Run a task at a specific time (once).
    ///
    /// If the timestamp is in the past, the task executes immediately.
    pub fn at<F, Fut>(self, timestamp: std::time::SystemTime, task: F) -> JobHandle
    where
        F: FnOnce() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move {
            let now = std::time::SystemTime::now();
            let delay = match timestamp.duration_since(now) {
                Ok(duration) => duration,
                Err(_) => Duration::from_secs(0),
            };
            tokio::select! {
                _ = tokio::time::sleep(delay) => {
                    task().await;
                }
                _ = cancel_clone.cancelled() => {}
            }
        });
        JobHandle { _handle: handle, cancel }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scheduler_after() {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f = flag.clone();

        let _handle = Scheduler::new().after(Duration::from_millis(50), move || {
            let f = f.clone();
            async move { f.store(true, std::sync::atomic::Ordering::Relaxed); }
        });

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_scheduler_at_past() {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f = flag.clone();

        let past = std::time::SystemTime::now() - Duration::from_secs(10);
        let _handle = Scheduler::new().at(past, move || {
            let f = f.clone();
            async move { f.store(true, std::sync::atomic::Ordering::Relaxed); }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        // Past time should execute immediately or skip
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
    }
}
