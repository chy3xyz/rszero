//! rszero: A cloud-native microservices framework for the Rust ecosystem.
//!
//! # Overview
//!
//! rszero is a microservices framework built on Axum (REST) and Volo (RPC),
//! combining modern Rust's memory safety and performance with battle-tested
//! distributed systems patterns.
//!
//! # Quick Start
//!
//! ```no_run
//! use rszero::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = load_config("etc/api.yaml")?;
//!     log::init(&config.log);
//!     let server = RszeroServer::from_config(&config);
//!     server.start().await?;
//!     Ok(())
//! }
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

// ─── Service Layer ────────────────────────────────────────────────────────

#[cfg(feature = "rest")]
/// REST API gateway built on Axum.
pub mod rest;
#[cfg(feature = "rpc")]
/// RPC service layer built on Volo (gRPC/Thrift).
pub mod rpc;

// ─── Infrastructure Layer ─────────────────────────────────────────────────

/// Multi-environment configuration (figment + dotenvy).
pub mod config;
/// Structured logging via tracing.
pub mod log;
#[cfg(feature = "cache")]
/// Redis cache layer (fred).
pub mod cache;
#[cfg(feature = "cache")]
/// In-process LRU/TTL cache.
pub mod cachex;
#[cfg(feature = "queue")]
/// Message queue (lapin).
pub mod queue;
#[cfg(feature = "store")]
/// Database/ORM (sqlx + sea-orm).
pub mod store;

// ─── Service Governance Layer ─────────────────────────────────────────────

#[cfg(feature = "discovery")]
/// Service discovery (etcd/nacos).
pub mod discovery;
#[cfg(feature = "rest")]
/// Rate limiting (tower-governor).
pub mod limit;
/// Circuit breaker.
pub mod breaker;
/// Load shedding — drops excess traffic under overload.
pub mod shedder;
/// Timeout enforcement for async operations.
pub mod timeout;
/// Retry with exponential backoff and jitter.
pub mod retry;

// ─── Cross-Cutting Concerns ───────────────────────────────────────────────

#[cfg(feature = "rest")]
/// Middleware (JWT, logging).
pub mod middleware;
#[cfg(feature = "trace")]
/// OpenTelemetry tracing.
pub mod trace;
/// Concurrency utilities (MapReduce, fx pipeline).
pub mod concurrent;
/// Health check (liveness/readiness).
pub mod health;
#[cfg(feature = "metrics")]
/// Prometheus metrics.
pub mod metrics;
/// OpenAPI specification generation.
pub mod openapi;
/// Saga distributed transaction coordinator.
pub mod saga;
#[cfg(feature = "trace")]
/// Trace context propagation.
pub use trace::propagation;
/// `.api` file parser (compatible with go-zero syntax).
pub mod api;
#[cfg(feature = "dashboard")]
/// Lightweight built-in monitoring dashboard (HTMX + Alpine.js + Tailwind).
pub mod dashboard;
/// Task scheduler for periodic and delayed jobs.
pub mod scheduler;
/// Background job worker.
#[cfg(feature = "queue")]
pub mod worker;
/// A/B testing and feature flags.
pub mod experiment;

// ─── Core Utilities ───────────────────────────────────────────────────────

/// Global error handling.
pub mod error;
/// Utility helpers.
pub mod utils;

/// One-shot import for framework users: `use rszero::prelude::*;`
pub mod prelude {
    pub use super::config::{RszeroConfig, LogConfig, CacheConfig, StoreConfig, RpcConfig, QueueConfig, DiscoveryConfig, load_config, load_config_with_env, ConfigWatcher, MultiConfigWatcher};
    pub use super::error::*;
    pub use super::log::{self, info, warn, error as log_error, debug, trace_span};

    #[cfg(feature = "rest")]
    pub use super::rest::{RszeroServer, RouteGroup, CorsConfig, JsonResponse, ok_response, error_response, Handler, FnHandler};
    #[cfg(feature = "rest")]
    pub use super::rest::context::RequestContext;
    #[cfg(feature = "rest")]
    pub use super::rest::param::{PathParam, QueryParam, ParamError, validate_required, validate_range};
    #[cfg(feature = "rpc")]
    pub use super::rpc::client::RpcClient;
    #[cfg(feature = "rpc")]
    pub use super::rpc::server::RpcServer;
    #[cfg(feature = "rpc")]
    pub use super::rpc::interceptor::{InterceptorChain, RpcContext, LoggingInterceptor, MetricsInterceptor, TimeoutInterceptor, RetryInterceptor};
    #[cfg(feature = "cache")]
    pub use super::cache::Cache;
    #[cfg(feature = "cache")]
    pub use super::cachex::Cache as MemCache;
    #[cfg(feature = "queue")]
    pub use super::queue::Queue;
    #[cfg(all(feature = "queue", feature = "cache"))]
    pub use super::queue::streams::RedisStreamsBackend;
    #[cfg(feature = "store")]
    pub use super::store::{Store, ReplicaStore, execute_batch, chunk_vec, BatchResult, DEFAULT_BATCH_SIZE};
    #[cfg(feature = "rest")]
    pub use super::middleware::{LogMiddleware, AccessLog, body_size_limit, validation_middleware, request_id_middleware, get_request_id, REQUEST_ID_HEADER, GlobalErrorHandler, ErrorBody, cache_middleware, ResponseCache, IdempotencyConfig, IdempotencyLayer, MemoryIdempotencyStore, IdempotencyStore, SignatureConfig, SignatureVerifier, compute_signature, CanaryConfig, CanaryFlag, CanaryLayer, TenantConfig, TenantId, TenantLayer, RequestSchema, FieldRule};
    #[cfg(all(feature = "rest", feature = "auth"))]
    pub use super::middleware::JwtMiddleware;
    #[cfg(feature = "discovery")]
    pub use super::discovery::{ServiceDiscovery, LoadBalanceStrategy};
    #[cfg(feature = "rest")]
    pub use super::limit::{rate_limiter, custom_rate_limiter};
    pub use super::breaker::{CircuitBreaker, BreakerConfig, BreakerState};
    pub use super::utils::*;
    pub use super::concurrent::{mr, fx};
    pub use super::shedder::AdaptiveShedder;
    pub use super::timeout::TimeoutExt;
    pub use super::health::{Health, DependencyHealth};
    #[cfg(feature = "metrics")]
    pub use super::metrics::Metrics;
    pub use super::openapi::OpenApiSpec;
    pub use super::saga::{Saga, SagaResult};
    #[cfg(feature = "store")]
    pub use super::saga::persistence::{PersistentSaga, SqlSagaPersister, SagaPersister, SagaRecord, SagaState};
    pub use super::scheduler::cron::{CronExpr, CronScheduler};
    #[cfg(feature = "trace")]
    pub use super::trace::propagation::{TraceContext, inject_http, extract_http, TRACEPARENT_HEADER};
    #[cfg(feature = "rest")]
    pub use super::rest::AckConfig;
    pub use super::api::{parse_api, parse_file, ApiFile, ApiType, ApiField, ApiService, ApiRoute, RouteMethod, generate, GeneratedCode};
    pub use super::scheduler::{Scheduler, JobHandle};
    #[cfg(feature = "queue")]
    pub use super::worker::{Worker, JobConfig, execute_with_retry};
    pub use super::retry::{RetryPolicy, with_retry};
    #[cfg(feature = "store")]
    pub use super::store::sharding::{ShardingStore, ShardStrategy};
    #[cfg(feature = "cache")]
    pub use super::cache::redlock::{RedlockManager, RedlockConfig, RedlockGuard, with_redlock};
    #[cfg(all(feature = "queue", feature = "store"))]
    pub use super::queue::transactional::{TransactionalQueue, MessageTable, MessageStatus, MessageTableStats};
    #[cfg(feature = "metrics")]
    pub use super::metrics::business::{BusinessMetrics, CounterRef, GaugeRef, HistogramRef};
    pub use super::experiment::{Experiment, ExperimentRegistry, Variant, ExperimentExposure};
    #[cfg(feature = "rest")]
    pub use super::rest::mock::{MockServer, MockConfig};
    #[cfg(feature = "rest")]
    pub use super::rest::upload::{UploadConfig, FileUpload, save_upload, save_multipart, file_download_response, file_info};
}
