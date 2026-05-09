//! Handler trait for request processing.

use std::future::Future;
use std::pin::Pin;
use axum::response::Response;

/// Trait for request handlers.
#[allow(clippy::async_yields_async)]
#[async_trait::async_trait]
pub trait Handler: Send + Sync {
    /// Process a request and return a response.
    fn handle(&self, req: axum::http::Request<axum::body::Body>) -> Pin<Box<dyn Future<Output = Response> + Send + '_>>;
}

/// Wrapper for async functions as handlers.
pub struct FnHandler<F> {
    func: F,
}

impl<F> FnHandler<F>
where
    F: Fn(axum::http::Request<axum::body::Body>) -> Pin<Box<dyn Future<Output = Response> + Send>>
        + Send + Sync,
{
    /// Create a new handler from an async function.
    pub fn new(func: F) -> Self { Self { func } }
}

impl<F> Handler for FnHandler<F>
where
    F: Fn(axum::http::Request<axum::body::Body>) -> Pin<Box<dyn Future<Output = Response> + Send>>
        + Send + Sync,
{
    fn handle(&self, req: axum::http::Request<axum::body::Body>) -> Pin<Box<dyn Future<Output = Response> + Send + '_>> {
        (self.func)(req)
    }
}
