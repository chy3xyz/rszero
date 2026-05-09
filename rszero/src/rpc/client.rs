//! RPC client with Volo gRPC integration, service discovery, timeout, and retry.
//!
//! The RpcClient provides configuration for connecting to Volo-based gRPC services.
//! Users generate their service stubs using `volo-build`, then use this client
//! to configure discovery, timeouts, and retry policies.
//!
//! # Example
//!
//! ```no_run
//! use rszero::rpc::RpcClient;
//! use rszero::config::RpcConfig;
//!
//! let client = RpcClient::new(RpcConfig::default());
//! ```

use std::time::Duration;
use crate::config::RpcConfig;
use crate::discovery::{ServiceDiscovery, LoadBalanceStrategy, ServiceInstance};
use crate::error::{RszeroError, RszeroResult};

/// RPC client configuration builder.
pub struct RpcClientBuilder {
    config: RpcConfig,
    timeout: Duration,
    max_retries: u32,
    discovery: Option<std::sync::Arc<ServiceDiscovery>>,
    load_balance: LoadBalanceStrategy,
}

impl RpcClientBuilder {
    /// Create a new builder from config.
    pub fn new(config: RpcConfig) -> Self {
        Self {
            config,
            timeout: Duration::from_secs(5),
            max_retries: 0,
            discovery: None,
            load_balance: LoadBalanceStrategy::RoundRobin,
        }
    }

    /// Set the request timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the maximum number of retries.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set the service discovery client.
    pub fn discovery(mut self, discovery: ServiceDiscovery) -> Self {
        self.discovery = Some(std::sync::Arc::new(discovery));
        self
    }

    /// Set the load balancing strategy.
    pub fn load_balance(mut self, strategy: LoadBalanceStrategy) -> Self {
        self.load_balance = strategy;
        self
    }

    /// Build the RPC client.
    pub fn build(self) -> RpcClient {
        RpcClient {
            config: self.config,
            timeout: self.timeout,
            max_retries: self.max_retries,
            discovery: self.discovery,
            load_balance: self.load_balance,
        }
    }
}

/// RPC client for calling remote Volo gRPC services.
#[derive(Clone)]
pub struct RpcClient {
    config: RpcConfig,
    timeout: Duration,
    max_retries: u32,
    discovery: Option<std::sync::Arc<ServiceDiscovery>>,
    load_balance: LoadBalanceStrategy,
}

impl RpcClient {
    /// Create a client from [`RpcConfig`].
    pub fn new(config: RpcConfig) -> Self {
        RpcClientBuilder::new(config).build()
    }

    /// Create a builder for fine-grained configuration.
    pub fn builder(config: RpcConfig) -> RpcClientBuilder {
        RpcClientBuilder::new(config)
    }

    /// Create a client configured for etcd service discovery.
    pub fn from_etcd(hosts: Vec<String>, key: String) -> Self {
        Self {
            config: RpcConfig {
                name: String::new(),
                listen_on: String::new(),
                etcd: Some(crate::config::EtcdConfig { hosts, key }),
            },
            timeout: Duration::from_secs(5),
            max_retries: 0,
            discovery: None,
            load_balance: LoadBalanceStrategy::RoundRobin,
        }
    }

    /// Discover a service instance using the configured load balancing strategy.
    pub async fn discover_instance(&self, service: &str) -> RszeroResult<Option<ServiceInstance>> {
        match &self.discovery {
            Some(discovery) => {
                discovery.select(service, self.load_balance).await
            }
            None => {
                // Fallback: if etcd config is present, create a temporary discovery client
                if let Some(etcd) = &self.config.etcd {
                    let discovery = ServiceDiscovery::from_etcd(etcd.hosts.clone());
                    discovery.connect().await?;
                    discovery.select(service, self.load_balance).await
                } else {
                    Err(RszeroError::Discovery { message: "no discovery configured".into(), source: None })
                }
            }
        }
    }

    /// Execute a gRPC call with timeout and retry.
    ///
    /// `call` is a closure that takes a `&str` address and returns the gRPC result.
    /// The client handles service discovery, timeout, and retry automatically.
    pub async fn call<F, Fut, T>(&self, service: &str, call: F) -> RszeroResult<T>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<T, RszeroError>>,
    {
        let instance = self.discover_instance(service).await?;
        let addr = instance
            .map(|i| i.addr)
            .ok_or_else(|| RszeroError::Discovery { message: format!("no instance found for service: {}", service), source: None })?;

        let mut last_error = None;
        for attempt in 0..=self.max_retries {
            match tokio::time::timeout(self.timeout, call(addr.clone())).await {
                Ok(Ok(result)) => return Ok(result),
                Ok(Err(e)) => {
                    tracing::warn!(attempt, error = %e, "rpc call failed");
                    last_error = Some(e);
                }
                Err(_) => {
                    tracing::warn!(attempt, "rpc call timed out");
                    last_error = Some(RszeroError::Rpc { message: "timeout".into(), source: None });
                }
            }

            if attempt < self.max_retries {
                let delay = Duration::from_millis(100 * (1 << attempt));
                tokio::time::sleep(delay.min(Duration::from_secs(5))).await;
            }
        }

        Err(last_error.unwrap_or_else(|| RszeroError::Rpc { message: "all retries exhausted".into(), source: None }))
    }

    /// Get the configured timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Get the configured max retries.
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// Get the service discovery client.
    pub fn discovery(&self) -> Option<&ServiceDiscovery> {
        self.discovery.as_deref()
    }

    /// Get the load balancing strategy.
    pub fn load_balance(&self) -> LoadBalanceStrategy {
        self.load_balance
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
    fn test_client_builder() {
        let config = RpcConfig::default();
        let client = RpcClient::builder(config)
            .timeout(Duration::from_secs(10))
            .max_retries(3)
            .load_balance(LoadBalanceStrategy::Random)
            .build();

        assert_eq!(client.timeout(), Duration::from_secs(10));
        assert_eq!(client.max_retries(), 3);
        assert_eq!(client.load_balance(), LoadBalanceStrategy::Random);
    }

    #[test]
    fn test_client_defaults() {
        let config = RpcConfig::default();
        let client = RpcClient::new(config);
        assert_eq!(client.timeout(), Duration::from_secs(5));
        assert_eq!(client.max_retries(), 0);
        assert!(client.discovery().is_none());
    }

    #[test]
    fn test_client_from_etcd() {
        let client = RpcClient::from_etcd(vec!["127.0.0.1:2379".into()], "test.rpc".into());
        assert!(client.config().etcd.is_some());
    }

    #[tokio::test]
    async fn test_client_call_timeout() {
        let client = RpcClient::builder(RpcConfig::default())
            .timeout(Duration::from_millis(10))
            .build();

        // Should fail because no discovery is configured
        let result = client.call("test-service", |_addr| async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok::<_, RszeroError>(42)
        }).await;

        assert!(result.is_err());
    }
}
