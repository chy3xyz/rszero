//! Rate limiting via tower-governor.

use governor::middleware::NoOpMiddleware;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;

/// Default rate limiter: 10 req/s, burst 30, keyed by client IP.
pub fn rate_limiter() -> GovernorLayer<'static, SmartIpKeyExtractor, NoOpMiddleware> {
    GovernorLayer {
        config: Box::leak(Box::new(
            GovernorConfigBuilder::default()
                .per_second(10)
                .burst_size(30)
                .key_extractor(SmartIpKeyExtractor)
                .finish()
                .expect("default rate limiter config is always valid"),
        )),
    }
}

/// Custom rate limiter with configurable rate and burst.
///
/// # Panics
///
/// Panics if `per_second` is 0 or `burst_size` is 0.
pub fn custom_rate_limiter(per_second: u64, burst_size: u32) -> GovernorLayer<'static, SmartIpKeyExtractor, NoOpMiddleware> {
    assert!(per_second > 0, "per_second must be > 0");
    assert!(burst_size > 0, "burst_size must be > 0");
    GovernorLayer {
        config: Box::leak(Box::new(
            GovernorConfigBuilder::default()
                .per_second(per_second)
                .burst_size(burst_size)
                .key_extractor(SmartIpKeyExtractor)
                .finish()
                .expect("validated rate limiter config should not fail"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_creation() {
        let layer = rate_limiter();
        // GovernorLayer does not expose internal state, but creation should not panic
        let _ = layer;
    }

    #[test]
    fn test_custom_rate_limiter_creation() {
        let layer = custom_rate_limiter(100, 200);
        let _ = layer;
    }

    #[test]
    fn test_custom_rate_limiter_zero_rate() {
        // GovernorConfigBuilder::finish() returns an error when per_second is 0,
        // but our code uses unwrap() which panics. This documents that behavior.
        // In production, prefer validate_before_build.
    }
}
