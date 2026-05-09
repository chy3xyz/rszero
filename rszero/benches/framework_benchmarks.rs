use criterion::{criterion_group, criterion_main, Criterion};
use rszero::prelude::*;
use std::time::Duration;

fn bench_error_creation(c: &mut Criterion) {
    c.bench_function("error/not_found", |b| {
        b.iter(|| RszeroError::not_found("test".to_string()))
    });

    c.bench_function("error/internal", |b| {
        b.iter(|| RszeroError::internal("test".to_string()))
    });

    c.bench_function("error_response/serialization", |b| {
        b.iter(|| {
            let err = RszeroError::not_found("test".to_string());
            ErrorResponse::from_error(&err)
        })
    });
}

fn bench_circuit_breaker(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("breaker/record_failure", |b| {
        let breaker = CircuitBreaker::with_count_threshold(100, 60);
        b.iter(|| rt.block_on(breaker.record_failure()))
    });

    c.bench_function("breaker/record_success", |b| {
        let breaker = CircuitBreaker::with_count_threshold(100, 60);
        b.iter(|| rt.block_on(breaker.record_success()))
    });
}

fn bench_load_shedder(c: &mut Criterion) {
    c.bench_function("shedder/record_latency", |b| {
        let shedder = AdaptiveShedder::new(100);
        b.iter(|| shedder.record_latency(50))
    });

    c.bench_function("shedder/should_reject", |b| {
        let shedder = AdaptiveShedder::new(100);
        b.iter(|| shedder.should_reject())
    });
}

fn bench_cache_operations(c: &mut Criterion) {
    c.bench_function("memcache/set", |b| {
        let cache: MemCache<String, i64> = MemCache::new(1000);
        b.iter(|| cache.set("key".to_string(), 42))
    });

    c.bench_function("memcache/get", |b| {
        let cache: MemCache<String, i64> = MemCache::new(1000);
        cache.set("key".to_string(), 42);
        b.iter(|| cache.get(&"key".to_string()))
    });
}

fn bench_utility_functions(c: &mut Criterion) {
    c.bench_function("utils/generate_id", |b| {
        b.iter(generate_id)
    });

    c.bench_function("utils/generate_short_id", |b| {
        b.iter(generate_short_id)
    });

    c.bench_function("utils/now_timestamp", |b| {
        b.iter(now_timestamp)
    });
}

fn bench_retry_policy(c: &mut Criterion) {
    c.bench_function("retry/build_policy", |b| {
        b.iter(|| RetryPolicy::new().max_retries(3).initial_delay(Duration::from_millis(100)))
    });
}

criterion_group!(
    benches,
    bench_error_creation,
    bench_circuit_breaker,
    bench_load_shedder,
    bench_cache_operations,
    bench_utility_functions,
    bench_retry_policy,
);
criterion_main!(benches);
