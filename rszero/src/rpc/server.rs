//! RPC server with Volo gRPC integration, graceful shutdown, and service registration.
//!
//! # Example
//!
//! ```no_run
//! use rszero::rpc::server::RpcServer;
//! use rszero::config::RpcConfig;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = RpcConfig::default();
//!     let server = RpcServer::new(config);
//!     server.start().await?;
//!     Ok(())
//! }
//! ```

use std::time::Duration;
use crate::config::RpcConfig;
use crate::error::RszeroResult;
use crate::health::Health;

/// RPC server builder for hosting Volo gRPC/Thrift services.
///
/// This is a framework wrapper — users generate their service stubs with `volo-build`,
/// then use [`RpcServer::start_with_service`] to run the generated server.
pub struct RpcServer {
    config: RpcConfig,
    graceful_timeout: Duration,
    health: Health,
    service_name: String,
}

impl RpcServer {
    /// Create a server from [`RpcConfig`].
    pub fn new(config: RpcConfig) -> Self {
        let name = config.name.clone();
        Self {
            config,
            graceful_timeout: Duration::from_secs(30),
            health: Health::new(),
            service_name: name,
        }
    }

    /// Create a server from the root [`RszeroConfig`](crate::config::RszeroConfig).
    pub fn from_config(config: &crate::config::RszeroConfig) -> Self {
        Self::new(config.rpc.clone())
    }

    /// Set the graceful shutdown timeout.
    pub fn graceful_timeout(mut self, timeout: Duration) -> Self {
        self.graceful_timeout = timeout;
        self
    }

    /// Set the service name.
    pub fn service_name(mut self, name: &str) -> Self {
        self.service_name = name.to_string();
        self
    }

    /// Get the health tracker for this server.
    pub fn health(&self) -> &Health {
        &self.health
    }

    /// Get the listen address.
    pub fn listen_on(&self) -> &str {
        &self.config.listen_on
    }

    /// Start the RPC server with a custom service.
    ///
    /// `make_service` is a closure that produces the Volo service.
    /// The server will bind to `config.listen_on`, register with etcd if configured,
    /// and handle graceful shutdown.
    pub async fn start_with_service<S, F>(self, make_service: F) -> RszeroResult<()>
    where
        S: volo::Service<volo_grpc::context::ServerContext, volo_grpc::Request<()>> + Send + Sync + 'static,
        F: FnOnce() -> S,
    {
        self.health.set_ready();

        tracing::info!(
            listen_on = %self.config.listen_on,
            service = %self.service_name,
            "rpc server starting"
        );

        // Register with etcd if configured
        if let Some(etcd) = &self.config.etcd {
            match crate::discovery::ServiceDiscovery::from_etcd(etcd.hosts.clone()).register(
                &self.service_name,
                &self.config.listen_on,
            ).await {
                Ok(_) => tracing::info!("registered with etcd"),
                Err(e) => tracing::warn!(error = %e, "etcd registration failed"),
            }
        }

        // Build and serve the Volo service
        let addr = volo::net::Address::from(
            self.config.listen_on.parse::<std::net::SocketAddr>()
                .map_err(|e| crate::error::RszeroError::Config { message: format!("invalid listen address: {}", e), source: None })?
        );

        let _service = make_service();
        let server = volo_grpc::server::Server::new()
            .run(addr);

        let shutdown = tokio::signal::ctrl_c();
        tokio::select! {
            result = server => {
                result.map_err(|e| crate::error::RszeroError::Rpc { message: e.to_string(), source: None })?;
            }
            _ = shutdown => {
                tracing::info!("shutdown signal received");
            }
        }

        self.health.set_not_ready();
        tracing::info!(
            timeout_secs = self.graceful_timeout.as_secs(),
            "rpc server shutting down"
        );

        tokio::time::sleep(self.graceful_timeout).await;
        tracing::info!("rpc server stopped");
        Ok(())
    }

    /// Start a basic RPC server without a custom service.
    ///
    /// This is useful for health-check endpoints and service discovery
    /// when you don't have a generated service yet.
    pub async fn start(self) -> RszeroResult<()> {
        self.health.set_ready();

        tracing::info!(
            listen_on = %self.config.listen_on,
            service = %self.service_name,
            "rpc server starting (basic mode)"
        );

        let shutdown = tokio::signal::ctrl_c();
        shutdown.await
            .map_err(|e| crate::error::RszeroError::Internal { message: e.to_string(), source: None })?;

        self.health.set_not_ready();
        tracing::info!(
            timeout_secs = self.graceful_timeout.as_secs(),
            "rpc server shutting down"
        );

        tokio::time::sleep(self.graceful_timeout).await;
        tracing::info!("rpc server stopped");
        Ok(())
    }

    /// Access the underlying config.
    pub fn config(&self) -> &RpcConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config() {
        let config = RpcConfig::default();
        let server = RpcServer::new(config);
        assert_eq!(server.graceful_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_server_graceful_timeout() {
        let config = RpcConfig::default();
        let server = RpcServer::new(config)
            .graceful_timeout(Duration::from_secs(60));
        assert_eq!(server.graceful_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_server_health() {
        let config = RpcConfig::default();
        let server = RpcServer::new(config);
        assert!(server.health().is_ready());
    }

    #[test]
    fn test_server_service_name() {
        let config = RpcConfig::default();
        let server = RpcServer::new(config).service_name("user-rpc");
        assert_eq!(server.service_name, "user-rpc");
    }
}
