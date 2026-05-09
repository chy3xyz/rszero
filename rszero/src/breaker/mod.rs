//! Production-grade circuit breaker with half-open state and adaptive recovery.
//!
//! Implements the standard circuit breaker pattern with three states:
//! - **Closed**: Normal operation, failures are counted.
//! - **Open**: Requests are rejected immediately after failure threshold is reached.
//! - **Half-Open**: After a cooldown period, a limited number of probe requests
//!   are allowed to test if the downstream service has recovered.
//!
//! # State Transitions
//!
//! ```text
//! Closed --(failure rate > threshold)--> Open
//! Open   --(cooldown expires)---------> HalfOpen
//! HalfOpen --(probes succeed)---------> Closed
//! HalfOpen --(probes fail)------------> Open
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Circuit breaker state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    /// Normal operation — requests pass through.
    Closed,
    /// Failing fast — all requests are rejected.
    Open,
    /// Testing recovery — limited probe requests allowed.
    HalfOpen,
}

/// Sliding window entry recording the outcome of a single request.
#[derive(Debug, Clone, Copy)]
struct WindowEntry {
    timestamp: Instant,
    success: bool,
}

/// Configuration for the circuit breaker.
#[derive(Debug, Clone, Copy)]
pub struct BreakerConfig {
    /// Failure rate threshold (0.0–1.0) that triggers open state.
    pub failure_threshold: f64,
    /// Minimum number of requests in the window before evaluating failure rate.
    pub min_requests: u64,
    /// Duration to stay in Open state before transitioning to HalfOpen.
    pub cooldown: Duration,
    /// Max probe requests allowed in HalfOpen state.
    pub max_probes: u64,
    /// Sliding window duration for failure rate calculation.
    pub window: Duration,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 0.5,
            min_requests: 10,
            cooldown: Duration::from_secs(30),
            max_probes: 5,
            window: Duration::from_secs(60),
        }
    }
}

/// Production-grade circuit breaker.
///
/// Thread-safe via internal locking on the sliding window and state.
pub struct CircuitBreaker {
    config: BreakerConfig,
    state: RwLock<BreakerState>,
    opened_at: RwLock<Option<Instant>>,
    window: RwLock<Vec<WindowEntry>>,
    probe_count: AtomicU64,
    total_success: AtomicU64,
    total_failure: AtomicU64,
}

impl CircuitBreaker {
    /// Create a circuit breaker with the given configuration.
    pub fn new(config: BreakerConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            state: RwLock::new(BreakerState::Closed),
            opened_at: RwLock::new(None),
            window: RwLock::new(Vec::with_capacity(config.min_requests as usize * 2)),
            probe_count: AtomicU64::new(0),
            total_success: AtomicU64::new(0),
            total_failure: AtomicU64::new(0),
        })
    }

    /// Create a simple circuit breaker with a fixed failure count threshold.
    ///
    /// This is a convenience constructor for simple use-cases. For production
    /// services prefer [`Self::new`] with a sliding window configuration.
    pub fn with_count_threshold(failure_threshold: u32, cooldown_secs: u64) -> Arc<Self> {
        Self::new(BreakerConfig {
            failure_threshold: 1.0,
            min_requests: failure_threshold as u64,
            cooldown: Duration::from_secs(cooldown_secs),
            max_probes: 3,
            window: Duration::from_secs(300),
        })
    }

    /// Get the current state.
    pub async fn state(&self) -> BreakerState {
        *self.state.read().await
    }

    /// Check if the circuit breaker is currently open (rejecting requests).
    pub async fn is_open(&self) -> bool {
        self.evaluate_state().await == BreakerState::Open
    }

    /// Check if the circuit breaker is closed (allowing requests).
    pub async fn is_closed(&self) -> bool {
        self.evaluate_state().await == BreakerState::Closed
    }

    /// Total successful requests recorded since creation.
    pub fn total_success(&self) -> u64 {
        self.total_success.load(Ordering::Relaxed)
    }

    /// Total failed requests recorded since creation.
    pub fn total_failure(&self) -> u64 {
        self.total_failure.load(Ordering::Relaxed)
    }

    /// Record a successful request outcome.
    pub async fn record_success(&self) {
        self.evaluate_state().await; // auto-transition Open -> HalfOpen
        self.append_window(true).await;
        self.total_success.fetch_add(1, Ordering::Relaxed);

        let mut state = self.state.write().await;
        if *state == BreakerState::HalfOpen {
            let probes = self.probe_count.fetch_add(1, Ordering::Relaxed) + 1;
            if probes >= self.config.max_probes {
                tracing::info!("circuit breaker closed after successful probes");
                *state = BreakerState::Closed;
                self.probe_count.store(0, Ordering::Relaxed);
            }
        }
    }

    /// Record a failed request outcome.
    pub async fn record_failure(&self) {
        self.evaluate_state().await; // auto-transition Open -> HalfOpen
        self.append_window(false).await;
        self.total_failure.fetch_add(1, Ordering::Relaxed);

        let mut state = self.state.write().await;
        match *state {
            BreakerState::Closed => {
                if self.should_open().await {
                    tracing::warn!(
                        threshold = self.config.failure_threshold,
                        "circuit breaker opened due to failure rate"
                    );
                    *state = BreakerState::Open;
                    *self.opened_at.write().await = Some(Instant::now());
                }
            }
            BreakerState::HalfOpen => {
                tracing::warn!("circuit breaker re-opened after probe failure");
                *state = BreakerState::Open;
                *self.opened_at.write().await = Some(Instant::now());
                self.probe_count.store(0, Ordering::Relaxed);
            }
            BreakerState::Open => {}
        }
    }

    /// Manually reset the breaker to closed state.
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        *state = BreakerState::Closed;
        *self.opened_at.write().await = None;
        self.window.write().await.clear();
        self.probe_count.store(0, Ordering::Relaxed);
        tracing::info!("circuit breaker manually reset");
    }

    /// Execute a fallible future through the circuit breaker.
    ///
    /// Returns `RszeroError::CircuitBreaker` immediately if the breaker is open.
    pub async fn execute<F, T, E>(&self, f: F) -> Result<T, crate::error::RszeroError>
    where
        F: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        if self.is_open().await {
            return Err(crate::error::RszeroError::CircuitBreaker);
        }

        // In half-open state, only allow probe_count requests through
        if self.evaluate_state().await == BreakerState::HalfOpen {
            let probe = self.probe_count.fetch_add(1, Ordering::Relaxed);
            if probe >= self.config.max_probes {
                return Err(crate::error::RszeroError::CircuitBreaker);
            }
        }

        match f.await {
            Ok(val) => {
                self.record_success().await;
                Ok(val)
            }
            Err(e) => {
                self.record_failure().await;
                Err(crate::error::RszeroError::Rpc { message: e.to_string(), source: None })
            }
        }
    }

    // ─── Internal helpers ───────────────────────────────────────────────────

    async fn evaluate_state(&self) -> BreakerState {
        let state = *self.state.read().await;
        if state != BreakerState::Open {
            return state;
        }

        let opened = *self.opened_at.read().await;
        if let Some(t) = opened {
            if t.elapsed() >= self.config.cooldown {
                let mut s = self.state.write().await;
                if *s == BreakerState::Open {
                    tracing::info!("circuit breaker entering half-open state");
                    *s = BreakerState::HalfOpen;
                    self.probe_count.store(0, Ordering::Relaxed);
                }
                return BreakerState::HalfOpen;
            }
        }
        BreakerState::Open
    }

    async fn append_window(&self, success: bool) {
        let mut window = self.window.write().await;
        let now = Instant::now();
        let cutoff = now - self.config.window;
        window.retain(|e| e.timestamp >= cutoff);
        window.push(WindowEntry { timestamp: now, success });
    }

    async fn should_open(&self) -> bool {
        let window = self.window.read().await;
        if window.len() < self.config.min_requests as usize {
            return false;
        }
        let failures = window.iter().filter(|e| !e.success).count();
        let rate = failures as f64 / window.len() as f64;
        rate >= self.config.failure_threshold
    }
}

// CircuitBreaker is intentionally NOT Clone.
// It must always be shared via Arc to preserve state across callers.

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_breaker_starts_closed() {
        let cb = CircuitBreaker::new(BreakerConfig::default());
        assert!(cb.is_closed().await);
        assert!(!cb.is_open().await);
    }

    #[tokio::test]
    async fn test_breaker_opens_on_failures() {
        let cb = CircuitBreaker::new(BreakerConfig {
            failure_threshold: 0.5,
            min_requests: 4,
            cooldown: Duration::from_secs(60),
            max_probes: 3,
            window: Duration::from_secs(60),
        });

        cb.record_failure().await;
        cb.record_failure().await;
        assert!(!cb.is_open().await); // not enough samples

        cb.record_failure().await;
        cb.record_failure().await; // 4 failures / 4 total = 100%
        assert!(cb.is_open().await);
    }

    #[tokio::test]
    async fn test_breaker_half_open_after_cooldown() {
        let cb = CircuitBreaker::new(BreakerConfig {
            failure_threshold: 0.5,
            min_requests: 2,
            cooldown: Duration::from_millis(50),
            max_probes: 3,
            window: Duration::from_secs(60),
        });

        cb.record_failure().await;
        cb.record_failure().await;
        assert!(cb.is_open().await);

        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(cb.evaluate_state().await, BreakerState::HalfOpen);
    }

    #[tokio::test]
    async fn test_breaker_closes_after_probes() {
        let cb = CircuitBreaker::new(BreakerConfig {
            failure_threshold: 0.5,
            min_requests: 2,
            cooldown: Duration::from_millis(10),
            max_probes: 2,
            window: Duration::from_secs(60),
        });

        cb.record_failure().await;
        cb.record_failure().await;
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Now half-open
        cb.record_success().await;
        cb.record_success().await;
        assert!(cb.is_closed().await);
    }

    #[tokio::test]
    async fn test_execute_blocks_when_open() {
        let cb = CircuitBreaker::new(BreakerConfig {
            failure_threshold: 0.5,
            min_requests: 1,
            cooldown: Duration::from_secs(60),
            max_probes: 3,
            window: Duration::from_secs(60),
        });

        cb.record_failure().await;
        let result = cb.execute(async { Ok::<_, &'static str>(42) }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reset() {
        let cb = CircuitBreaker::new(BreakerConfig {
            failure_threshold: 0.5,
            min_requests: 1,
            cooldown: Duration::from_secs(60),
            max_probes: 3,
            window: Duration::from_secs(60),
        });

        cb.record_failure().await;
        assert!(cb.is_open().await);
        cb.reset().await;
        assert!(cb.is_closed().await);
    }
}
