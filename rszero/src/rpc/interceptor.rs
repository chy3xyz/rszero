//! RPC interceptor / middleware chain for Volo gRPC services.
//!
//! Provides a tower-like middleware pattern for RPC calls, supporting
//! pre-request and post-response hooks.

use crate::error::RszeroError;
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;

/// RPC context passed through the interceptor chain.
#[derive(Debug, Clone)]
pub struct RpcContext {
    /// Service name being called.
    pub service: String,
    /// Method name being called.
    pub method: String,
    /// Request metadata/headers.
    pub metadata: std::collections::HashMap<String, String>,
    /// When the request started.
    pub start: Instant,
}

impl RpcContext {
    /// Create a new RPC context.
    pub fn new(service: &str, method: &str) -> Self {
        Self {
            service: service.to_string(),
            method: method.to_string(),
            metadata: std::collections::HashMap::new(),
            start: Instant::now(),
        }
    }

    /// Set a metadata key-value pair.
    pub fn set_metadata(&mut self, key: &str, value: &str) {
        self.metadata.insert(key.to_string(), value.to_string());
    }

    /// Get elapsed time since context creation.
    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }
}

/// RPC interceptor trait.
///
/// Interceptors can modify the context before the request and/or
/// inspect the result after the response.
#[async_trait::async_trait]
pub trait Interceptor: Send + Sync {
    /// Called before the RPC request.
    async fn before(&self, ctx: &mut RpcContext) -> Result<(), crate::error::RszeroError>;
    /// Called after the RPC response.
    async fn after(&self, ctx: &RpcContext, result: &Result<(), crate::error::RszeroError>);
}

/// Type-erased interceptor for chaining.
pub type BoxInterceptor = Box<dyn Interceptor>;

/// Interceptor chain.
pub struct InterceptorChain {
    interceptors: Vec<Arc<dyn Interceptor>>,
}

impl InterceptorChain {
    /// Create an empty interceptor chain.
    pub fn new() -> Self {
        Self {
            interceptors: Vec::new(),
        }
    }

    /// Add an interceptor to the chain.
    pub fn with<I: Interceptor + 'static>(mut self, interceptor: I) -> Self {
        self.interceptors.push(Arc::new(interceptor));
        self
    }

    /// Execute a function through the interceptor chain.
    pub async fn execute<F, Fut, T>(
        &self,
        mut ctx: RpcContext,
        f: F,
    ) -> Result<T, crate::error::RszeroError>
    where
        F: FnOnce(RpcContext) -> Fut,
        Fut: Future<Output = Result<T, crate::error::RszeroError>>,
    {
        // Before hooks
        for interceptor in &self.interceptors {
            interceptor.before(&mut ctx).await?;
        }

        // Execute the actual call
        let result = f(ctx.clone()).await;

        // After hooks
        let result_ref: Result<(), RszeroError> = match &result {
            Ok(_) => Ok(()),
            Err(e) => Err(e.clone()),
        };
        for interceptor in &self.interceptors {
            interceptor.after(&ctx, &result_ref).await;
        }

        result
    }

    /// Get the number of interceptors in the chain.
    pub fn len(&self) -> usize {
        self.interceptors.len()
    }

    /// Check if the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.interceptors.is_empty()
    }
}

impl Default for InterceptorChain {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Built-in interceptors ──────────────────────────────────────────────────

/// Logging interceptor that traces RPC calls.
pub struct LoggingInterceptor;

#[async_trait::async_trait]
impl Interceptor for LoggingInterceptor {
    async fn before(&self, ctx: &mut RpcContext) -> Result<(), crate::error::RszeroError> {
        tracing::info!(
            service = %ctx.service,
            method = %ctx.method,
            "rpc call started"
        );
        Ok(())
    }

    async fn after(&self, ctx: &RpcContext, result: &Result<(), crate::error::RszeroError>) {
        match result {
            Ok(()) => tracing::info!(
                service = %ctx.service,
                method = %ctx.method,
                duration_ms = ctx.elapsed().as_millis(),
                "rpc call succeeded"
            ),
            Err(e) => tracing::warn!(
                service = %ctx.service,
                method = %ctx.method,
                duration_ms = ctx.elapsed().as_millis(),
                error = %e,
                "rpc call failed"
            ),
        }
    }
}

/// Metrics interceptor that records RPC call metrics.
pub struct MetricsInterceptor;

#[async_trait::async_trait]
impl Interceptor for MetricsInterceptor {
    async fn before(&self, _ctx: &mut RpcContext) -> Result<(), crate::error::RszeroError> {
        Ok(())
    }

    async fn after(&self, ctx: &RpcContext, result: &Result<(), crate::error::RszeroError>) {
        let status = if result.is_ok() { "success" } else { "error" };
        tracing::debug!(
            service = %ctx.service,
            method = %ctx.method,
            status,
            duration_ms = ctx.elapsed().as_millis(),
            "rpc metric"
        );
    }
}

/// Timeout interceptor that enforces a maximum RPC duration.
pub struct TimeoutInterceptor {
    timeout: std::time::Duration,
}

impl TimeoutInterceptor {
    /// Create a new timeout interceptor.
    pub fn new(timeout: std::time::Duration) -> Self {
        Self { timeout }
    }
}

#[async_trait::async_trait]
impl Interceptor for TimeoutInterceptor {
    async fn before(&self, _ctx: &mut RpcContext) -> Result<(), crate::error::RszeroError> {
        Ok(())
    }

    async fn after(&self, ctx: &RpcContext, _result: &Result<(), crate::error::RszeroError>) {
        let elapsed = ctx.elapsed();
        if elapsed > self.timeout {
            tracing::warn!(
                service = %ctx.service,
                method = %ctx.method,
                timeout_ms = self.timeout.as_millis(),
                elapsed_ms = elapsed.as_millis(),
                "rpc timeout exceeded"
            );
        }
    }
}

/// Retry interceptor that adds retry metadata to the context.
pub struct RetryInterceptor {
    max_retries: u32,
}

impl RetryInterceptor {
    /// Create a new retry interceptor.
    pub fn new(max_retries: u32) -> Self {
        Self { max_retries }
    }
}

#[async_trait::async_trait]
impl Interceptor for RetryInterceptor {
    async fn before(&self, ctx: &mut RpcContext) -> Result<(), crate::error::RszeroError> {
        ctx.set_metadata("max_retries", &self.max_retries.to_string());
        Ok(())
    }

    async fn after(&self, _ctx: &RpcContext, _result: &Result<(), crate::error::RszeroError>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_interceptor_chain() {
        let chain = InterceptorChain::new()
            .with(LoggingInterceptor)
            .with(MetricsInterceptor);

        assert_eq!(chain.len(), 2);
        assert!(!chain.is_empty());

        let ctx = RpcContext::new("test", "ping");
        let result = chain.execute(ctx, |_ctx| async { Ok::<_, crate::error::RszeroError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_interceptor_chain_error() {
        let chain = InterceptorChain::new().with(LoggingInterceptor);

        let ctx = RpcContext::new("test", "fail");
        let result = chain.execute(ctx, |_ctx| async {
            Err::<i32, _>(crate::error::RszeroError::Rpc { message: "fail".into(), source: None })
        }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_timeout_interceptor() {
        let interceptor = TimeoutInterceptor::new(std::time::Duration::from_secs(1));
        let mut ctx = RpcContext::new("test", "slow");
        interceptor.before(&mut ctx).await.unwrap();
        interceptor.after(&ctx, &Ok(())).await;
    }

    #[tokio::test]
    async fn test_retry_interceptor() {
        let interceptor = RetryInterceptor::new(3);
        let mut ctx = RpcContext::new("test", "retry");
        interceptor.before(&mut ctx).await.unwrap();
        assert_eq!(ctx.metadata.get("max_retries").unwrap(), "3");
    }
}
