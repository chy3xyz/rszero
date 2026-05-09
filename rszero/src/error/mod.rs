//! Global unified error handling for rszero.
//!
//! Provides [`RszeroError`] as the single error type used across all modules.
//! When the `rest` feature is enabled, also provides HTTP response conversion.
//!
//! # Error Chain
//!
//! RszeroError preserves the original [`source`] error for external failures
//! (config, database, cache, RPC, IO, etc.), enabling full error-chain
//! inspection via [`std::error::Error::source`].

use serde::Serialize;
use std::sync::Arc;
use thiserror::Error;

/// Type alias for boxed error sources.
pub type ErrorSource = Arc<dyn std::error::Error + Send + Sync + 'static>;

/// Unified error type for all rszero operations.
#[derive(Debug, Error)]
pub enum RszeroError {
    /// Configuration loading or parsing error.
    #[error("config error: {message}")]
    Config {
        /// Human-readable message.
        message: String,
        /// Original error source, if any.
        #[source]
        source: Option<ErrorSource>,
    },

    /// Database operation error.
    #[error("database error: {message}")]
    Database {
        /// Human-readable message.
        message: String,
        /// Original error source, if any.
        #[source]
        source: Option<ErrorSource>,
    },

    /// Redis cache operation error.
    #[error("cache error: {message}")]
    Cache {
        /// Human-readable message.
        message: String,
        /// Original error source, if any.
        #[source]
        source: Option<ErrorSource>,
    },

    /// RPC call error.
    #[error("rpc error: {message}")]
    Rpc {
        /// Human-readable message.
        message: String,
        /// Original error source, if any.
        #[source]
        source: Option<ErrorSource>,
    },

    /// HTTP error with status code and message.
    #[error("http error: {code} {msg}")]
    Http {
        /// HTTP status code.
        code: u16,
        /// Error message.
        msg: String,
    },

    /// Authentication or authorization error.
    #[error("auth error: {message}")]
    Auth {
        /// Human-readable message.
        message: String,
        /// Original error source, if any.
        #[source]
        source: Option<ErrorSource>,
    },

    /// Rate limit exceeded.
    #[error("rate limit exceeded")]
    RateLimit,

    /// Circuit breaker is open, request rejected.
    #[error("circuit breaker open")]
    CircuitBreaker,

    /// Resource not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Service discovery error.
    #[error("service discovery error: {message}")]
    Discovery {
        /// Human-readable message.
        message: String,
        /// Original error source, if any.
        #[source]
        source: Option<ErrorSource>,
    },

    /// Message queue operation error.
    #[error("queue error: {message}")]
    Queue {
        /// Human-readable message.
        message: String,
        /// Original error source, if any.
        #[source]
        source: Option<ErrorSource>,
    },

    /// JSON serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Internal server error.
    #[error("internal error: {message}")]
    Internal {
        /// Human-readable message.
        message: String,
        /// Original error source, if any.
        #[source]
        source: Option<ErrorSource>,
    },
}

impl Clone for RszeroError {
    fn clone(&self) -> Self {
        match self {
            Self::Config { message, source } => Self::Config {
                message: message.clone(),
                source: source.clone(),
            },
            Self::Database { message, source } => Self::Database {
                message: message.clone(),
                source: source.clone(),
            },
            Self::Cache { message, source } => Self::Cache {
                message: message.clone(),
                source: source.clone(),
            },
            Self::Rpc { message, source } => Self::Rpc {
                message: message.clone(),
                source: source.clone(),
            },
            Self::Http { code, msg } => Self::Http {
                code: *code,
                msg: msg.clone(),
            },
            Self::Auth { message, source } => Self::Auth {
                message: message.clone(),
                source: source.clone(),
            },
            Self::RateLimit => Self::RateLimit,
            Self::CircuitBreaker => Self::CircuitBreaker,
            Self::NotFound(s) => Self::NotFound(s.clone()),
            Self::Discovery { message, source } => Self::Discovery {
                message: message.clone(),
                source: source.clone(),
            },
            Self::Queue { message, source } => Self::Queue {
                message: message.clone(),
                source: source.clone(),
            },
            Self::Serialization(e) => {
                let io_err = std::io::Error::other(e.to_string());
                Self::Serialization(serde_json::Error::io(io_err))
            }
            Self::Internal { message, source } => Self::Internal {
                message: message.clone(),
                source: source.clone(),
            },
        }
    }
}

impl RszeroError {
    /// Create a config error without a source.
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config {
            message: msg.into(),
            source: None,
        }
    }

    /// Create a config error with a source.
    pub fn config_with_source(msg: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Config {
            message: msg.into(),
            source: Some(Arc::new(source)),
        }
    }

    /// Create a database error without a source.
    pub fn database(msg: impl Into<String>) -> Self {
        Self::Database {
            message: msg.into(),
            source: None,
        }
    }

    /// Create a database error with a source.
    pub fn database_with_source(msg: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Database {
            message: msg.into(),
            source: Some(Arc::new(source)),
        }
    }

    /// Create a cache error without a source.
    pub fn cache(msg: impl Into<String>) -> Self {
        Self::Cache {
            message: msg.into(),
            source: None,
        }
    }

    /// Create a cache error with a source.
    pub fn cache_with_source(msg: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Cache {
            message: msg.into(),
            source: Some(Arc::new(source)),
        }
    }

    /// Create an RPC error without a source.
    pub fn rpc(msg: impl Into<String>) -> Self {
        Self::Rpc {
            message: msg.into(),
            source: None,
        }
    }

    /// Create an RPC error with a source.
    pub fn rpc_with_source(msg: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Rpc {
            message: msg.into(),
            source: Some(Arc::new(source)),
        }
    }

    /// Create an HTTP error with status code.
    pub fn http(code: u16, msg: impl Into<String>) -> Self {
        Self::Http {
            code,
            msg: msg.into(),
        }
    }

    /// Create an auth error without a source.
    pub fn auth(msg: impl Into<String>) -> Self {
        Self::Auth {
            message: msg.into(),
            source: None,
        }
    }

    /// Create an auth error with a source.
    pub fn auth_with_source(msg: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Auth {
            message: msg.into(),
            source: Some(Arc::new(source)),
        }
    }

    /// Create a service discovery error without a source.
    pub fn discovery(msg: impl Into<String>) -> Self {
        Self::Discovery {
            message: msg.into(),
            source: None,
        }
    }

    /// Create a discovery error with a source.
    pub fn discovery_with_source(msg: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Discovery {
            message: msg.into(),
            source: Some(Arc::new(source)),
        }
    }

    /// Create a queue error without a source.
    pub fn queue(msg: impl Into<String>) -> Self {
        Self::Queue {
            message: msg.into(),
            source: None,
        }
    }

    /// Create a queue error with a source.
    pub fn queue_with_source(msg: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Queue {
            message: msg.into(),
            source: Some(Arc::new(source)),
        }
    }

    /// Create a not found error.
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// Create an internal server error without a source.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal {
            message: msg.into(),
            source: None,
        }
    }

    /// Create an internal error with a source.
    pub fn internal_with_source(msg: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal {
            message: msg.into(),
            source: Some(Arc::new(source)),
        }
    }

    /// Convert to HTTP status code.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Http { code, .. } => *code,
            Self::Auth { .. } => 401,
            Self::RateLimit => 429,
            Self::CircuitBreaker => 503,
            Self::NotFound(..) => 404,
            _ => 500,
        }
    }

    /// Numeric error code for go-zero compatibility.
    pub fn code(&self) -> i32 {
        match self {
            Self::Http { code, .. } => *code as i32,
            Self::Auth { .. } => 401,
            Self::RateLimit => 429,
            Self::CircuitBreaker => 503,
            Self::NotFound(..) => 404,
            _ => 500,
        }
    }
}

impl From<figment::Error> for RszeroError {
    fn from(e: figment::Error) -> Self {
        Self::Config {
            message: e.to_string(),
            source: Some(Arc::new(e)),
        }
    }
}

impl From<std::io::Error> for RszeroError {
    fn from(e: std::io::Error) -> Self {
        Self::Internal {
            message: e.to_string(),
            source: Some(Arc::new(e)),
        }
    }
}

#[cfg(feature = "store")]
impl From<sqlx::Error> for RszeroError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database {
            message: e.to_string(),
            source: Some(Arc::new(e)),
        }
    }
}

/// Go-zero compatible error response: `{ "code": N, "msg": "..." }`.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error code (HTTP-compatible).
    pub code: i32,
    /// Human-readable error message.
    pub msg: String,
}

impl ErrorResponse {
    /// Create a new error response.
    pub fn new(code: i32, msg: impl Into<String>) -> Self {
        Self {
            code,
            msg: msg.into(),
        }
    }

    /// Create from an [`RszeroError`].
    pub fn from_error(err: &RszeroError) -> Self {
        Self {
            code: err.code(),
            msg: err.to_string(),
        }
    }
}

/// Result type alias using [`RszeroError`].
pub type RszeroResult<T> = Result<T, RszeroError>;

// ─── Axum integration (only when `rest` feature is enabled) ───────────────

#[cfg(feature = "rest")]
mod rest_integration {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::Json;

    impl RszeroError {
        /// Convert to HTTP status code (axum).
        pub fn to_status_code(&self) -> StatusCode {
            StatusCode::from_u16(self.status_code())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }

    impl IntoResponse for ErrorResponse {
        fn into_response(self) -> Response {
            let status = if self.code >= 500 {
                StatusCode::INTERNAL_SERVER_ERROR
            } else if self.code >= 400 {
                StatusCode::from_u16(self.code as u16).unwrap_or(StatusCode::BAD_REQUEST)
            } else {
                StatusCode::OK
            };
            (status, Json(self)).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;

    #[test]
    fn test_error_codes() {
        assert_eq!(RszeroError::auth("x").code(), 401);
        assert_eq!(RszeroError::RateLimit.code(), 429);
        assert_eq!(RszeroError::CircuitBreaker.code(), 503);
        assert_eq!(RszeroError::NotFound("x".into()).code(), 404);
        assert_eq!(
            RszeroError::Internal {
                message: "x".into(),
                source: None
            }
            .code(),
            500
        );
        assert_eq!(RszeroError::http(418, "teapot").code(), 418);
    }

    #[test]
    fn test_status_codes() {
        assert_eq!(RszeroError::auth("x").status_code(), 401);
        assert_eq!(RszeroError::RateLimit.status_code(), 429);
        assert_eq!(RszeroError::CircuitBreaker.status_code(), 503);
        assert_eq!(RszeroError::NotFound("x".into()).status_code(), 404);
        assert_eq!(
            RszeroError::Internal {
                message: "x".into(),
                source: None
            }
            .status_code(),
            500
        );
    }

    #[test]
    fn test_error_response() {
        let resp = ErrorResponse::new(404, "not found");
        assert_eq!(resp.code, 404);
        assert_eq!(resp.msg, "not found");

        let err = RszeroError::cache("connection refused");
        let resp = ErrorResponse::from_error(&err);
        assert_eq!(resp.code, 500);
        assert!(resp.msg.contains("cache"));
    }

    #[test]
    fn test_from_conversions() {
        let io_err = std::io::Error::other("io error");
        let rszero: RszeroError = io_err.into();
        assert!(matches!(
            rszero,
            RszeroError::Internal {
                message: _,
                source: Some(_)
            }
        ));
        assert!(rszero.source().is_some());

        let figment_err = figment::Error::from("missing key");
        let rszero: RszeroError = figment_err.into();
        assert!(matches!(
            rszero,
            RszeroError::Config {
                message: _,
                source: Some(_)
            }
        ));
        assert!(rszero.source().is_some());
    }

    #[test]
    fn test_display() {
        let err = RszeroError::config("bad yaml");
        assert_eq!(format!("{}", err), "config error: bad yaml");

        let err = RszeroError::http(503, "unavailable");
        assert_eq!(format!("{}", err), "http error: 503 unavailable");
    }

    #[test]
    fn test_error_chain_preservation() {
        let inner = std::io::Error::other("connection refused");
        let err = RszeroError::database_with_source("query failed", inner);
        assert!(err.source().is_some());
        assert!(err.to_string().contains("query failed"));
    }
}
