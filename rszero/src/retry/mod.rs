//! Retry mechanism with exponential backoff and jitter.
//!
//! Provides configurable retry policies for resilient service communication.

use std::time::Duration;
use std::future::Future;

/// Retry policy configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct RetryPolicy {
    max_retries: u32,
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
    jitter: bool,
}

impl RetryPolicy {
    /// Create a new retry policy with defaults: 3 retries, 100ms initial delay, 5s max delay.
    pub fn new() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            multiplier: 2.0,
            jitter: true,
        }
    }

    /// Set the maximum number of retries.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set the initial delay between retries.
    pub fn initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Set the maximum delay between retries.
    pub fn max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set the exponential backoff multiplier.
    pub fn multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// Enable or disable jitter.
    pub fn jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;
        self
    }

    /// Get the maximum number of retries.
    pub fn get_max_retries(&self) -> u32 {
        self.max_retries
    }

    /// Get the initial delay.
    pub fn get_initial_delay(&self) -> Duration {
        self.initial_delay
    }

    /// Get the maximum delay.
    pub fn get_max_delay(&self) -> Duration {
        self.max_delay
    }

    /// Calculate the delay for a given attempt number.
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base = self.initial_delay.as_millis() as f64
            * self.multiplier.powi(attempt as i32);
        let delay_ms = base.min(self.max_delay.as_millis() as f64) as u64;

        if self.jitter && delay_ms >= 2 {
            let jitter_ms = fastrand::u64(0..delay_ms / 2);
            Duration::from_millis(delay_ms + jitter_ms)
        } else {
            Duration::from_millis(delay_ms)
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self { Self::new() }
}

/// Execute a fallible async operation with retry.
pub async fn with_retry<F, Fut, T, E>(
    policy: &RetryPolicy,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;

    for attempt in 0..=policy.max_retries {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(e) => {
                tracing::warn!(attempt, error = %e, "operation failed, retrying");
                last_error = Some(e);

                if attempt < policy.max_retries {
                    let delay = policy.delay_for_attempt(attempt);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    Err(last_error.expect("last_error must be set: loop executes at least once and sets it on failure"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_retry_succeeds_eventually() {
        let counter = AtomicU32::new(0);
        let policy = RetryPolicy::new()
            .max_retries(3)
            .initial_delay(Duration::from_millis(1))
            .jitter(false);

        let result = with_retry(&policy, || async {
            let count = counter.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                Err("not yet")
            } else {
                Ok("success")
            }
        }).await;

        assert_eq!(result, Ok("success"));
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let counter = AtomicU32::new(0);
        let policy = RetryPolicy::new()
            .max_retries(2)
            .initial_delay(Duration::from_millis(1))
            .jitter(false);

        let result = with_retry(&policy, || async {
            counter.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>("always fails")
        }).await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_delay_calculation() {
        let policy = RetryPolicy::new()
            .initial_delay(Duration::from_millis(100))
            .multiplier(2.0)
            .jitter(false);

        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(400));
    }

    #[test]
    fn test_delay_max_cap() {
        let policy = RetryPolicy::new()
            .initial_delay(Duration::from_millis(100))
            .max_delay(Duration::from_millis(500))
            .multiplier(10.0)
            .jitter(false);

        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(500));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(500));
    }

    #[test]
    fn test_delay_zero_no_panic() {
        // Regression test: delay_ms=0 should not panic with fastrand::u64(0..0)
        let policy = RetryPolicy::new()
            .initial_delay(Duration::from_millis(0))
            .max_delay(Duration::from_millis(0))
            .jitter(true);
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(0));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(0));
    }

    #[test]
    fn test_delay_one_ms_jitter() {
        // delay_ms=1 with jitter should not panic (delay_ms/2 = 0)
        let policy = RetryPolicy::new()
            .initial_delay(Duration::from_millis(1))
            .max_delay(Duration::from_millis(1))
            .jitter(true);
        let d = policy.delay_for_attempt(0);
        assert!(d >= Duration::from_millis(1));
    }
}
