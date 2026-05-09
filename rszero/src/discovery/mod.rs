//! Service discovery with etcd backend, watch mechanism, and load balancing.
//!
//! Provides service registration, deregistration, discovery via etcd key-value store
//! with automatic lease-based TTL for health checking, plus client-side load balancing.

use crate::config::DiscoveryConfig;
use crate::error::{RszeroError, RszeroResult};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Load balancing strategy for selecting service instances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadBalanceStrategy {
    /// Round-robin selection.
    #[default]
    RoundRobin,
    /// Random selection.
    Random,
    /// Always pick the first available instance.
    First,
}

/// Registered service instance.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceInstance {
    /// Service name.
    pub name: String,
    /// Service address (host:port).
    pub addr: String,
    /// Metadata key-value pairs.
    pub metadata: std::collections::HashMap<String, String>,
    /// Registration timestamp.
    pub registered_at: String,
    /// Weight for weighted load balancing.
    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_weight() -> u32 { 100 }

/// Handle for a service registration that automatically keeps the lease alive.
/// When dropped, the keep-alive task is cancelled.
pub struct RegistrationHandle {
    _cancel_tx: tokio::sync::oneshot::Sender<()>,
}

/// Service discovery client supporting etcd.
pub struct ServiceDiscovery {
    config: DiscoveryConfig,
    client: Arc<RwLock<Option<etcd_client::Client>>>,
    lease_id: Arc<RwLock<Option<i64>>>,
    round_robin_counter: Arc<RwLock<usize>>,
}

impl ServiceDiscovery {
    /// Create a new discovery client from config.
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            config,
            client: Arc::new(RwLock::new(None)),
            lease_id: Arc::new(RwLock::new(None)),
            round_robin_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Create an etcd-based discovery client.
    pub fn from_etcd(hosts: Vec<String>) -> Self {
        Self::new(DiscoveryConfig {
            kind: "etcd".into(),
            endpoints: hosts,
        })
    }

    /// Connect to the etcd cluster.
    pub async fn connect(&self) -> RszeroResult<()> {
        if self.config.kind != "etcd" {
            return Err(RszeroError::Discovery { message: format!(
                "unsupported discovery backend: {}",
                self.config.kind
            ), source: None });
        }

        let endpoints = &self.config.endpoints;
        if endpoints.is_empty() {
            return Err(RszeroError::Discovery { message: "no etcd endpoints configured".into(), source: None });
        }

        let client = etcd_client::Client::connect(endpoints, None)
            .await
            .map_err(|e| RszeroError::Discovery { message: format!("failed to connect to etcd: {}", e), source: None })?;

        *self.client.write().await = Some(client);
        Ok(())
    }

    /// Register a service instance with etcd and start automatic lease keep-alive.
    ///
    /// Returns a [`RegistrationHandle`] that keeps the lease alive in the background.
    /// When the handle is dropped, the keep-alive task stops.
    pub async fn register(&self, service: &str, addr: &str) -> RszeroResult<RegistrationHandle> {
        self.ensure_connected().await?;

        let mut client_guard = self.client.write().await;
        let client = client_guard.as_mut()
            .ok_or_else(|| RszeroError::Discovery { message: "etcd client not connected".into(), source: None })?;

        let lease_resp = client.lease_grant(10, None).await
            .map_err(|e| RszeroError::Discovery { message: format!("failed to grant lease: {}", e), source: None })?;

        let lease_id = lease_resp.id();
        *self.lease_id.write().await = Some(lease_id);

        let key = format!("/rszero/services/{}/{}", service, addr);
        let value = serde_json::json!({
            "name": service,
            "addr": addr,
            "registered_at": chrono::Utc::now().to_rfc3339(),
            "weight": 100,
        }).to_string();

        let put_opts = etcd_client::PutOptions::new().with_lease(lease_id);
        client.put(key, value, Some(put_opts))
            .await
            .map_err(|e| RszeroError::Discovery { message: format!("failed to register service: {}", e), source: None })?;

        drop(client_guard);

        // Clone the Arc for the background task
        let client_arc = self.client.clone();
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let mut guard = client_arc.write().await;
                        if let Some(client) = guard.as_mut() {
                            if let Ok((mut keeper, _)) = client.lease_keep_alive(lease_id).await {
                                if keeper.keep_alive().await.is_err() {
                                    tracing::warn!("etcd keep-alive failed");
                                    break;
                                }
                            }
                        }
                    }
                    _ = &mut cancel_rx => {
                        tracing::debug!("registration keep-alive cancelled");
                        break;
                    }
                }
            }
        });

        tracing::info!(service, addr, "service registered with auto keep-alive");
        Ok(RegistrationHandle { _cancel_tx: cancel_tx })
    }

    /// Deregister a service instance.
    pub async fn deregister(&self, service: &str, addr: &str) -> RszeroResult<()> {
        let key = format!("/rszero/services/{}/{}", service, addr);

        let mut client_guard = self.client.write().await;
        let client = client_guard.as_mut()
            .ok_or_else(|| RszeroError::Discovery { message: "etcd client not connected".into(), source: None })?;

        client.delete(key, None)
            .await
            .map_err(|e| RszeroError::Discovery { message: format!("failed to deregister service: {}", e), source: None })?;

        tracing::info!(service, addr, "service deregistered");
        Ok(())
    }

    /// Discover all instances of a service.
    pub async fn discover(&self, service: &str) -> RszeroResult<Vec<ServiceInstance>> {
        let prefix = format!("/rszero/services/{}/", service);

        let mut client_guard = self.client.write().await;
        let client = client_guard.as_mut()
            .ok_or_else(|| RszeroError::Discovery { message: "etcd client not connected".into(), source: None })?;

        let get_opts = etcd_client::GetOptions::new().with_prefix();
        let resp = client.get(prefix, Some(get_opts))
            .await
            .map_err(|e| RszeroError::Discovery { message: format!("failed to discover service: {}", e), source: None })?;

        let instances: Vec<ServiceInstance> = resp.kvs()
            .iter()
            .filter_map(|kv| {
                let value = std::str::from_utf8(kv.value()).ok()?;
                serde_json::from_str::<ServiceInstance>(value).ok()
            })
            .collect();

        Ok(instances)
    }

    /// Select a single instance using the given load balancing strategy.
    pub async fn select(&self, service: &str, strategy: LoadBalanceStrategy) -> RszeroResult<Option<ServiceInstance>> {
        let instances = self.discover(service).await?;
        if instances.is_empty() {
            return Ok(None);
        }

        match strategy {
            LoadBalanceStrategy::RoundRobin => {
                let mut counter = self.round_robin_counter.write().await;
                let idx = *counter % instances.len();
                *counter += 1;
                Ok(Some(instances[idx].clone()))
            }
            LoadBalanceStrategy::Random => {
                let idx = fastrand::usize(0..instances.len());
                Ok(Some(instances[idx].clone()))
            }
            LoadBalanceStrategy::First => Ok(Some(instances[0].clone())),
        }
    }

    /// Watch for changes to a service and invoke the callback on updates.
    pub async fn watch<F>(&self, service: &str, mut callback: F) -> RszeroResult<()>
    where
        F: FnMut(Vec<ServiceInstance>) + Send + 'static,
    {
        self.ensure_connected().await?;

        let prefix = format!("/rszero/services/{}/", service);
        let mut client_guard = self.client.write().await;
        let client = client_guard.as_mut()
            .ok_or_else(|| RszeroError::Discovery { message: "etcd client not connected".into(), source: None })?;

        let (mut watcher, mut stream) = client.watch(prefix, Some(etcd_client::WatchOptions::new().with_prefix()))
            .await
            .map_err(|e| RszeroError::Discovery { message: format!("failed to start watch: {}", e), source: None })?;

        drop(client_guard); // release lock before await loop

        tokio::spawn(async move {
            while let Some(resp) = stream.message().await.ok().flatten() {
                let instances: Vec<ServiceInstance> = resp.events()
                    .iter()
                    .filter_map(|ev| {
                        if let Some(kv) = ev.kv() {
                            let value = std::str::from_utf8(kv.value()).ok()?;
                            serde_json::from_str::<ServiceInstance>(value).ok()
                        } else {
                            None
                        }
                    })
                    .collect();
                callback(instances);
            }
            let _ = watcher.cancel().await;
        });

        Ok(())
    }

    /// Keep the lease alive by sending periodic heartbeats.
    pub async fn keep_alive(&self) -> RszeroResult<()> {
        let lease_id = *self.lease_id.read().await;
        let lease_id = lease_id.ok_or_else(|| RszeroError::Discovery { message: "no lease to keep alive".into(), source: None })?;

        let mut client_guard = self.client.write().await;
        let client = client_guard.as_mut()
            .ok_or_else(|| RszeroError::Discovery { message: "etcd client not connected".into(), source: None })?;

        let (mut keeper, _) = client.lease_keep_alive(lease_id)
            .await
            .map_err(|e| RszeroError::Discovery { message: format!("failed to start keep alive: {}", e), source: None })?;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                if keeper.keep_alive().await.is_err() {
                    break;
                }
            }
        });

        Ok(())
    }

    /// Get the discovery backend kind.
    pub fn kind(&self) -> &str {
        &self.config.kind
    }

    /// Check if the discovery backend is connected and responsive.
    pub async fn is_healthy(&self) -> bool {
        if let Some(client) = self.client.write().await.as_mut() {
            client.status().await.is_ok()
        } else {
            false
        }
    }

    /// Start a background health probe that reports to the given [`Health`] tracker.
    pub fn monitor_health(self, health: crate::health::Health, interval: std::time::Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if self.is_healthy().await {
                    health.set_dependency("discovery", crate::health::DependencyHealth::Healthy).await;
                } else {
                    health.set_dependency("discovery", crate::health::DependencyHealth::Unhealthy("etcd unreachable".into())).await;
                }
            }
        })
    }

    async fn ensure_connected(&self) -> RszeroResult<()> {
        if self.client.read().await.is_none() {
            self.connect().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_config() {
        let discovery = ServiceDiscovery::from_etcd(vec!["127.0.0.1:2379".into()]);
        assert_eq!(discovery.kind(), "etcd");
    }

    #[tokio::test]
    async fn test_discovery_connect_fails_without_endpoints() {
        let discovery = ServiceDiscovery::new(DiscoveryConfig {
            kind: "etcd".into(),
            endpoints: vec![],
        });
        let result = discovery.connect().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_load_balance_strategy_default() {
        assert_eq!(LoadBalanceStrategy::default(), LoadBalanceStrategy::RoundRobin);
    }

    #[test]
    fn test_service_instance_default_weight() {
        let inst = ServiceInstance {
            name: "test".into(),
            addr: "127.0.0.1:8080".into(),
            metadata: std::collections::HashMap::new(),
            registered_at: chrono::Utc::now().to_rfc3339(),
            weight: default_weight(),
        };
        assert_eq!(inst.weight, 100);
    }
}
