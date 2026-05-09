//! Multi-environment configuration management.
//!
//! Uses figment for YAML/TOML/env loading and dotenvy for `.env` files.
//! Supports hot-reloading via the [`watcher`] module.

#![allow(clippy::field_reassign_with_default)]

pub mod types;
pub mod watcher;

pub use types::*;
pub use watcher::{ConfigWatcher, MultiConfigWatcher};

use figment::providers::Format;

/// Load configuration from a YAML file with environment variable overrides.
///
/// Reads `.env` file, then YAML config, then `RSZERO_` prefixed env vars.
pub fn load_config(path: &str) -> crate::error::RszeroResult<RszeroConfig> {
    use crate::error::RszeroError;
    dotenvy::dotenv().ok();
    let config: RszeroConfig = figment::Figment::new()
        .merge(figment::providers::Yaml::file(path))
        .merge(figment::providers::Env::prefixed("RSZERO_"))
        .extract()
        .map_err(|e| RszeroError::Config { message: e.to_string(), source: None })?;
    Ok(config)
}

/// Load configuration with a specific environment profile.
///
/// Tries `.env.{env}` first, then falls back to `.env`.
pub fn load_config_with_env(path: &str, env: &str) -> crate::error::RszeroResult<RszeroConfig> {
    use crate::error::RszeroError;
    dotenvy::from_filename(format!(".env.{}", env)).ok();
    dotenvy::dotenv().ok();
    let config: RszeroConfig = figment::Figment::new()
        .merge(figment::providers::Yaml::file(path))
        .merge(figment::providers::Env::prefixed("RSZERO_"))
        .extract()
        .map_err(|e| RszeroError::Config { message: e.to_string(), source: None })?;
    Ok(config)
}

impl RszeroConfig {
    /// Validate the configuration.
    ///
    /// Checks that required fields are set and values are within acceptable ranges.
    pub fn validate(&self) -> crate::error::RszeroResult<()> {
        use crate::error::RszeroError;

        if self.name.is_empty() {
            return Err(RszeroError::Config { message: "service name is required".into(), source: None });
        }
        if self.port == 0 {
            return Err(RszeroError::Config { message: "port must be non-zero".into(), source: None });
        }
        if !self.store.dsn.is_empty() {
            if self.store.max_connections == 0 {
                return Err(RszeroError::Config { message: "store max_connections must be > 0".into(), source: None });
            }
            if self.store.min_connections > self.store.max_connections {
                return Err(RszeroError::Config { message: "store min_connections cannot exceed max_connections".into(), source: None });
            }
        }
        if (self.queue.kind == "redis" || self.queue.kind == "redis-streams" || self.queue.kind == "rabbitmq")
            && self.queue.url.is_empty()
        {
            return Err(RszeroError::Config { message: format!("queue url is required for '{}' backend", self.queue.kind), source: None });
        }
        if self.discovery.kind == "etcd" && self.discovery.endpoints.is_empty() {
            return Err(RszeroError::Config { message: "discovery endpoints are required for etcd".into(), source: None });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RszeroConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.log.level, "info");
        assert_eq!(config.cache.host, "127.0.0.1");
        assert_eq!(config.cache.port, 6379);
        assert_eq!(config.queue.kind, "redis");
        assert_eq!(config.discovery.kind, "etcd");
    }

    #[test]
    fn test_config_defaults() {
        let config = RszeroConfig::default();
        assert!(config.rpc.etcd.is_none());
        assert!(config.cache.password.is_none());
        assert!(config.store.dsn.is_empty());
        assert!(config.queue.url.is_empty());
        assert!(config.discovery.endpoints.is_empty());
    }

    #[test]
    fn test_config_validate_ok() {
        let mut config = RszeroConfig::default();
        config.name = "test".into();
        config.port = 8080;
        config.queue.kind = "memory".into(); // avoid url check
        config.discovery.kind = "none".into(); // avoid endpoints check
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_empty_name() {
        let mut config = RszeroConfig::default();
        config.name = String::new();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_zero_port() {
        let mut config = RszeroConfig::default();
        config.port = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_store_pool() {
        let mut config = RszeroConfig::default();
        config.name = "test".into();
        config.store.dsn = "postgres://localhost/db".into();
        config.store.max_connections = 5;
        config.store.min_connections = 10;
        assert!(config.validate().is_err());
    }
}
