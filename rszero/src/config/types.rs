//! Configuration type definitions.

use serde::Deserialize;

/// Root configuration for an rszero service.
#[derive(Debug, Clone, Deserialize)]
pub struct RszeroConfig {
    /// Service name.
    #[serde(default = "default_name")]
    pub name: String,
    /// Bind host address.
    #[serde(default = "default_host")]
    pub host: String,
    /// Bind port.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Logging configuration.
    #[serde(default)]
    pub log: LogConfig,
    /// Redis cache configuration.
    #[serde(default)]
    pub cache: CacheConfig,
    /// Database connection configuration.
    #[serde(default)]
    pub store: StoreConfig,
    /// RPC service configuration.
    #[serde(default)]
    pub rpc: RpcConfig,
    /// Message queue configuration.
    #[serde(default)]
    pub queue: QueueConfig,
    /// Service discovery configuration.
    #[serde(default)]
    pub discovery: DiscoveryConfig,
}

/// Logging configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    /// Log level: trace, debug, info, warn, error.
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Log format: json or text.
    #[serde(default = "default_log_format")]
    pub format: String,
    /// Log output: stdout or file path.
    #[serde(default = "default_log_output")]
    pub output: String,
}

/// Redis cache configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    /// Redis host.
    #[serde(default = "default_cache_host")]
    pub host: String,
    /// Redis port.
    #[serde(default = "default_cache_port")]
    pub port: u16,
    /// Redis database index.
    #[serde(default)]
    pub db: u8,
    /// Redis password (optional).
    pub password: Option<String>,
    /// Connection pool size.
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,
}

/// Database connection configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct StoreConfig {
    /// Database connection string (DSN).
    #[serde(default)]
    pub dsn: String,
    /// Maximum connection pool size.
    #[serde(default = "default_max_conn")]
    pub max_connections: u32,
    /// Minimum connection pool size.
    #[serde(default = "default_min_conn")]
    pub min_connections: u32,
}

/// RPC service configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RpcConfig {
    /// Service name for registration.
    #[serde(default)]
    pub name: String,
    /// Listen address (host:port).
    #[serde(default = "default_rpc_listen")]
    pub listen_on: String,
    /// etcd registration config (optional).
    pub etcd: Option<EtcdConfig>,
}

/// etcd service registration configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct EtcdConfig {
    /// etcd server addresses.
    pub hosts: Vec<String>,
    /// Registration key prefix.
    #[serde(default)]
    pub key: String,
}

/// Message queue configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {
    /// Queue backend: redis, redis-streams, or rabbitmq.
    #[serde(default = "default_queue_kind")]
    pub kind: String,
    /// Queue connection URL.
    #[serde(default)]
    pub url: String,
    /// Consumer group name (for redis-streams).
    #[serde(default)]
    pub group: Option<String>,
}

/// Service discovery configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveryConfig {
    /// Discovery backend: etcd or nacos.
    #[serde(default = "default_discovery_kind")]
    pub kind: String,
    /// Discovery server endpoints.
    #[serde(default)]
    pub endpoints: Vec<String>,
}

fn default_name() -> String { "rszero-service".into() }
fn default_host() -> String { "0.0.0.0".into() }
fn default_port() -> u16 { 8080 }
fn default_log_level() -> String { "info".into() }
fn default_log_format() -> String { "json".into() }
fn default_log_output() -> String { "stdout".into() }
fn default_cache_host() -> String { "127.0.0.1".into() }
fn default_cache_port() -> u16 { 6379 }
fn default_pool_size() -> usize { 10 }
fn default_max_conn() -> u32 { 10 }
fn default_min_conn() -> u32 { 2 }
fn default_rpc_listen() -> String { "0.0.0.0:8081".into() }
fn default_queue_kind() -> String { "redis".into() }
fn default_discovery_kind() -> String { "etcd".into() }

impl Default for RszeroConfig {
    fn default() -> Self {
        Self {
            name: default_name(), host: default_host(), port: default_port(),
            log: LogConfig::default(), cache: CacheConfig::default(),
            store: StoreConfig::default(), rpc: RpcConfig::default(),
            queue: QueueConfig::default(), discovery: DiscoveryConfig::default(),
        }
    }
}
impl Default for LogConfig {
    fn default() -> Self {
        Self { level: default_log_level(), format: default_log_format(), output: default_log_output() }
    }
}
impl Default for CacheConfig {
    fn default() -> Self {
        Self { host: default_cache_host(), port: default_cache_port(), db: 0, password: None, pool_size: default_pool_size() }
    }
}
impl Default for StoreConfig {
    fn default() -> Self {
        Self { dsn: String::new(), max_connections: default_max_conn(), min_connections: default_min_conn() }
    }
}
impl Default for RpcConfig {
    fn default() -> Self {
        Self { name: String::new(), listen_on: default_rpc_listen(), etcd: None }
    }
}
impl Default for QueueConfig {
    fn default() -> Self {
        Self { kind: default_queue_kind(), url: String::new(), group: None }
    }
}
impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self { kind: default_discovery_kind(), endpoints: Vec::new() }
    }
}
