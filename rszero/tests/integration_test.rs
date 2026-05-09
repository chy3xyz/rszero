//! Integration tests for rszero framework.
//!
//! These tests verify end-to-end behavior across multiple modules.
//! Run with: cargo test --test '*'

#![allow(unused_imports)]

use rszero::prelude::*;

#[test]
fn test_config_defaults() {
    let config = RszeroConfig::default();
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, 8080);
    assert_eq!(config.log.level, "info");
}

#[test]
fn test_error_response_serialization() {
    let err = RszeroError::not_found("resource".to_string());
    let resp = ErrorResponse::from_error(&err);
    assert_eq!(resp.code, 404);
    assert!(resp.msg.contains("not found"));
}

#[tokio::test]
async fn test_circuit_breaker_lifecycle() {
    let breaker = CircuitBreaker::with_count_threshold(3, 60);
    assert!(!breaker.is_open().await);

    breaker.record_failure().await;
    breaker.record_failure().await;
    assert!(!breaker.is_open().await);

    breaker.record_failure().await;
    assert!(breaker.is_open().await);

    breaker.reset().await;
    assert!(!breaker.is_open().await);
}

#[test]
fn test_cache_ttl_expiration() {
    use std::time::Duration;
    let cache = MemCache::new(10);
    cache.set_with_ttl("key".to_string(), 42, Some(Duration::from_millis(50)));
    assert_eq!(cache.get(&"key".to_string()), Some(42));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(cache.get(&"key".to_string()), None);
}

#[test]
fn test_load_shedder() {
    let shedder = AdaptiveShedder::new(100);
    assert!(!shedder.is_active());

    for _ in 0..100 {
        shedder.record_latency(500);
    }
    assert!(shedder.is_active());

    shedder.deactivate();
    assert!(!shedder.is_active());
}

#[test]
fn test_utility_functions() {
    let id = generate_id();
    assert_eq!(id.len(), 36);

    let short_id = generate_short_id();
    assert_eq!(short_id.len(), 12);

    let ts = now_timestamp();
    assert!(ts > 1_700_000_000);
}

#[tokio::test]
async fn test_timeout_extension() {
    use rszero::timeout::TimeoutExt;
    use std::time::Duration;

    let result = async { 42 }.timeout(Duration::from_secs(1)).await;
    assert_eq!(result, Some(42));

    let result = async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        42
    }
    .timeout(Duration::from_millis(10))
    .await;
    assert!(result.is_none());
}

#[test]
fn test_retry_policy_defaults() {
    let policy = RetryPolicy::new();
    assert_eq!(policy.get_max_retries(), 3);
    assert_eq!(policy.get_initial_delay(), std::time::Duration::from_millis(100));
}

#[test]
fn test_retry_policy_custom() {
    let policy = RetryPolicy::new()
        .max_retries(5)
        .initial_delay(std::time::Duration::from_secs(1))
        .max_delay(std::time::Duration::from_secs(30))
        .multiplier(3.0)
        .jitter(false);

    assert_eq!(policy.get_max_retries(), 5);
    assert_eq!(policy.get_initial_delay(), std::time::Duration::from_secs(1));
    assert_eq!(policy.get_max_delay(), std::time::Duration::from_secs(30));
}

#[test]
fn test_health_check() {
    let health = Health::new();
    assert!(health.is_ready());

    health.set_not_ready();
    assert!(!health.is_ready());

    health.set_ready();
    assert!(health.is_ready());
}

#[test]
fn test_metrics_export() {
    let metrics = Metrics::new("test-service");
    metrics.record_request("GET", "/test", 200);
    metrics.record_error("database");

    let output = metrics.export_prometheus();
    assert!(output.contains("rszero_requests_total"));
    assert!(output.contains("rszero_errors_total"));
    assert!(output.contains("test-service"));
}

#[test]
fn test_fx_stream_pipeline() {
    let result = fx::from(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
        .filter(|x| x % 2 == 0)
        .map(|x| x * 10)
        .head(3)
        .done();

    assert_eq!(result, vec![20, 40, 60]);
}

#[tokio::test]
async fn test_map_reduce_concurrent() {
    use rszero::concurrent::mr::{map_reduce, MapResult};

    let items: Vec<i32> = (1..=100).collect();
    let sum: i64 = map_reduce(
        items,
        |item| Box::pin(async move { MapResult::Ok(item as i64) }),
        |results| results.into_iter().sum::<i64>(),
    )
    .await;

    assert_eq!(sum, 5050);
}

#[test]
fn test_rpc_client_builder() {
    use rszero::rpc::RpcClient;
    use std::time::Duration;

    let config = rszero::config::RpcConfig::default();
    let client = RpcClient::builder(config)
        .timeout(Duration::from_secs(30))
        .max_retries(5)
        .build();

    assert_eq!(client.timeout(), Duration::from_secs(30));
    assert_eq!(client.max_retries(), 5);
}
