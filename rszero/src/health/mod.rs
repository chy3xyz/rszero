//! Health check middleware for rszero services.
//!
//! Provides `/health` (liveness) and `/ready` (readiness) endpoints,
//! plus dependency health tracking for databases, caches, and external services.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Health status of a dependency.
#[derive(Debug, Clone)]
pub enum DependencyHealth {
    /// Dependency is healthy.
    Healthy,
    /// Dependency is unhealthy with reason.
    Unhealthy(String),
}

/// Health status tracker with dependency support.
#[derive(Clone)]
pub struct Health {
    ready: Arc<AtomicBool>,
    dependencies: Arc<RwLock<HashMap<String, DependencyHealth>>>,
}

impl Health {
    /// Create a new health tracker (starts as ready).
    pub fn new() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(true)),
            dependencies: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Mark the service as not ready.
    pub fn set_not_ready(&self) {
        self.ready.store(false, Ordering::SeqCst);
    }

    /// Mark the service as ready.
    pub fn set_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }

    /// Check if the service is ready.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// Register a dependency and its health status.
    pub async fn set_dependency(&self, name: &str, health: DependencyHealth) {
        let mut deps = self.dependencies.write().await;
        deps.insert(name.to_string(), health);
    }

    /// Get all dependency health statuses.
    pub async fn dependencies(&self) -> HashMap<String, DependencyHealth> {
        self.dependencies.read().await.clone()
    }

    /// Check if all dependencies are healthy.
    pub async fn all_dependencies_healthy(&self) -> bool {
        let deps = self.dependencies.read().await;
        deps.values().all(|h| matches!(h, DependencyHealth::Healthy))
    }

    /// Perform a full health check (self + dependencies).
    pub async fn full_check(&self) -> bool {
        self.is_ready() && self.all_dependencies_healthy().await
    }
}

impl Default for Health {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_starts_ready() {
        let health = Health::new();
        assert!(health.is_ready());
    }

    #[test]
    fn test_health_toggle() {
        let health = Health::new();
        health.set_not_ready();
        assert!(!health.is_ready());
        health.set_ready();
        assert!(health.is_ready());
    }

    #[tokio::test]
    async fn test_dependency_health() {
        let health = Health::new();
        health.set_dependency("db", DependencyHealth::Healthy).await;
        health.set_dependency("cache", DependencyHealth::Unhealthy("timeout".into())).await;

        assert!(!health.all_dependencies_healthy().await);
        assert!(!health.full_check().await);

        let deps = health.dependencies().await;
        assert_eq!(deps.len(), 2);
    }
}
