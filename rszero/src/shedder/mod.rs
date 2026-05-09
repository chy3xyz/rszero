//! Load shedding — drops excess traffic under overload.
//!
//! Replicates go-zero's adaptive load shedding based on system CPU and request latency.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Adaptive load shedder that rejects requests when system is overloaded.
///
/// Uses a probabilistic approach based on recent request latency and
/// a configurable threshold. When the system is under pressure, requests
/// are randomly rejected to prevent cascading failures.
pub struct AdaptiveShedder {
    /// Whether shedding is currently active.
    active: AtomicBool,
    /// Threshold for average latency (ms) above which shedding starts.
    latency_threshold_ms: AtomicU64,
    /// Rolling average of recent request latencies.
    avg_latency_ms: AtomicU64,
    /// Number of samples in the rolling window.
    sample_count: AtomicU64,
    /// Total latency sum for averaging.
    latency_sum: AtomicU64,
    /// Probability of rejecting a request (0-10000, representing 0-100%).
    reject_probability: AtomicU64,
}

impl AdaptiveShedder {
    /// Create a new shedder with the given latency threshold.
    pub fn new(latency_threshold_ms: u64) -> Self {
        Self {
            active: AtomicBool::new(false),
            latency_threshold_ms: AtomicU64::new(latency_threshold_ms),
            avg_latency_ms: AtomicU64::new(0),
            sample_count: AtomicU64::new(0),
            latency_sum: AtomicU64::new(0),
            reject_probability: AtomicU64::new(0),
        }
    }

    /// Record a request latency and update shedding state.
    pub fn record_latency(&self, latency_ms: u64) {
        let count = self.sample_count.fetch_add(1, Ordering::Relaxed) + 1;
        let sum = self.latency_sum.fetch_add(latency_ms, Ordering::Relaxed) + latency_ms;
        let avg = sum / count;
        self.avg_latency_ms.store(avg, Ordering::Relaxed);

        let threshold = self.latency_threshold_ms.load(Ordering::Relaxed);
        if avg > threshold {
            // Exponential increase in rejection probability
            let excess = (avg - threshold) as f64 / threshold as f64;
            let prob = (excess * 100.0).min(95.0) as u64 * 100;
            self.reject_probability.store(prob, Ordering::Relaxed);
            self.active.store(true, Ordering::Relaxed);
        } else {
            // Gradually decrease probability
            let current = self.reject_probability.load(Ordering::Relaxed);
            if current > 0 {
                self.reject_probability.store(current.saturating_sub(500), Ordering::Relaxed);
            }
            if self.reject_probability.load(Ordering::Relaxed) == 0 {
                self.active.store(false, Ordering::Relaxed);
            }
        }

        // Reset window periodically
        if count > 1000 {
            self.sample_count.store(0, Ordering::Relaxed);
            self.latency_sum.store(0, Ordering::Relaxed);
        }
    }

    /// Check if the current request should be rejected.
    pub fn should_reject(&self) -> bool {
        if !self.active.load(Ordering::Relaxed) {
            return false;
        }
        let prob = self.reject_probability.load(Ordering::Relaxed);
        if prob == 0 {
            return false;
        }
        fastrand::u64(0..10000) < prob
    }

    /// Manually activate shedding.
    pub fn activate(&self) {
        self.active.store(true, Ordering::Relaxed);
        self.reject_probability.store(5000, Ordering::Relaxed);
    }

    /// Manually deactivate shedding.
    pub fn deactivate(&self) {
        self.active.store(false, Ordering::Relaxed);
        self.reject_probability.store(0, Ordering::Relaxed);
    }

    /// Check if shedding is currently active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Get the current rejection probability (0-10000).
    pub fn reject_probability(&self) -> u64 {
        self.reject_probability.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_shedding_under_threshold() {
        let shedder = AdaptiveShedder::new(100);
        shedder.record_latency(50);
        shedder.record_latency(60);
        shedder.record_latency(70);
        assert!(!shedder.is_active());
    }

    #[test]
    fn test_shedding_activates_over_threshold() {
        let shedder = AdaptiveShedder::new(100);
        for _ in 0..100 {
            shedder.record_latency(500);
        }
        assert!(shedder.is_active());
        assert!(shedder.reject_probability() > 0);
    }

    #[test]
    fn test_manual_activate_deactivate() {
        let shedder = AdaptiveShedder::new(100);
        shedder.activate();
        assert!(shedder.is_active());
        shedder.deactivate();
        assert!(!shedder.is_active());
    }
}
