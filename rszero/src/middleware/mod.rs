//! Common middleware — JWT authentication, logging, distributed tracing, request ID, and validation.

/// JWT authentication middleware.
#[cfg(feature = "auth")]
pub mod jwt;
/// Request/response logging middleware.
pub mod log;
/// Structured access log middleware.
pub mod access_log;
/// Distributed tracing middleware.
pub mod trace;
/// Request body size limiting middleware.
pub mod limit;
/// Request ID correlation middleware.
pub mod request_id;
/// Request validation middleware.
pub mod validation;
/// HTTP response caching middleware.
pub mod cache;
/// Global error handling middleware.
pub mod error;
/// Idempotency key middleware.
pub mod idempotency;
/// Request signature verification middleware.
pub mod signature;
/// Canary release / gray deployment middleware.
pub mod canary;
/// JSON Schema validation middleware.
pub mod schema;
/// Multi-tenant middleware.
pub mod tenant;

#[cfg(feature = "auth")]
pub use jwt::JwtMiddleware;
pub use log::LogMiddleware;
pub use access_log::{AccessLog, access_log_middleware};
pub use trace::{trace_middleware, TRACE_ID_HEADER, SPAN_ID_HEADER, PARENT_SPAN_ID_HEADER, SAMPLED_HEADER};
pub use limit::body_size_limit;
pub use request_id::{request_id_middleware, REQUEST_ID_HEADER, get_request_id};
pub use validation::{ValidationRules, validation_middleware};
pub use cache::{cache_middleware, CacheConfig, ResponseCache};
pub use error::{error_middleware, GlobalErrorHandler, ErrorBody};
pub use idempotency::{idempotency_middleware, IdempotencyConfig, IdempotencyLayer, MemoryIdempotencyStore, IdempotencyStore};
pub use signature::{signature_middleware, SignatureConfig, SignatureVerifier, compute_signature};
pub use canary::{canary_middleware, CanaryConfig, CanaryFlag, CanaryLayer};
pub use schema::{schema_validation_middleware, RequestSchema, FieldRule};
pub use tenant::{tenant_middleware, TenantConfig, TenantId, TenantLayer};
