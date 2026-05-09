# rszero API 参考

> 本文档列出 rszero 框架所有公共模块的 API。

---

## 快速导入

```rust
use rszero::prelude::*;
```

Prelude 导出了所有常用类型和函数。

---

## 模块索引

| 模块 | 文档 |
|------|------|
| [rest](#rest) | REST API 网关 |
| [rpc](#rpc) | RPC 服务 |
| [config](#config) | 配置管理 |
| [log](#log) | 结构化日志 |
| [cache](#cache) | 缓存层 |
| [queue](#queue) | 消息队列 |
| [store](#store) | 数据库/ORM |
| [limit](#limit) | 限流 |
| [breaker](#breaker) | 熔断 |
| [discovery](#discovery) | 服务发现 |
| [retry](#retry) | 重试 |
| [shedder](#shedder) | 负载脱落 |
| [timeout](#timeout) | 超时 |
| [health](#health) | 健康检查 |
| [metrics](#metrics) | Prometheus 指标 |
| [openapi](#openapi) | OpenAPI 文档 |
| [middleware](#middleware) | 中间件 |
| [concurrent](#concurrent) | 并发工具 |
| [error](#error) | 错误处理 |
| [utils](#utils) | 工具函数 |

---

## rest

REST API 网关，基于 Axum 0.7。

### RszeroServer

```rust
pub struct RszeroServer { /* ... */ }

impl RszeroServer {
    pub fn new(host: &str, port: u16) -> Self;
    pub fn from_config(config: &RszeroConfig) -> Self;
    pub fn route(self, path: &str, method_router: MethodRouter) -> Self;
    pub fn merge_router(self, other: Router) -> Self;
    pub fn cors(self, config: CorsConfig) -> Self;
    pub fn compression(self) -> Self;
    pub async fn start(self) -> RszeroResult<()>;
    pub async fn start_with_shutdown(self, signal: Option<impl Future>) -> RszeroResult<()>;
    pub fn router(&self) -> &Router;
}
```

### CorsConfig

```rust
pub struct CorsConfig { /* ... */ }

impl CorsConfig {
    pub fn permissive() -> Self;
    pub fn allow_origins(self, origins: Vec<String>) -> Self;
    pub fn allow_methods(self, methods: Vec<String>) -> Self;
    pub fn build(self) -> CorsLayer;
}
```

### JsonResponse

```rust
pub struct JsonResponse<T: Serialize> {
    pub code: i32,
    pub msg: String,
    pub data: Option<T>,
}

impl<T: Serialize> JsonResponse<T> {
    pub fn ok(data: T) -> Self;
    pub fn error(code: i32, msg: impl Into<String>) -> Self;
}

impl JsonResponse<()> {
    pub fn ok_empty() -> Self;
}
```

---

## rpc

RPC 客户端和服务器。

### RpcClient

```rust
pub struct RpcClient { /* ... */ }

impl RpcClient {
    pub fn new(config: RpcConfig) -> Self;
    pub fn builder(config: RpcConfig) -> RpcClientBuilder;
    pub fn from_etcd(hosts: Vec<String>, key: String) -> Self;
    pub fn timeout(&self) -> Duration;
    pub fn max_retries(&self) -> u32;
    pub fn discovery(&self) -> Option<&ServiceDiscovery>;
    pub fn config(&self) -> &RpcConfig;
}
```

### RpcClientBuilder

```rust
pub struct RpcClientBuilder { /* ... */ }

impl RpcClientBuilder {
    pub fn new(config: RpcConfig) -> Self;
    pub fn timeout(self, timeout: Duration) -> Self;
    pub fn max_retries(self, retries: u32) -> Self;
    pub fn discovery(self, discovery: ServiceDiscovery) -> Self;
    pub fn build(self) -> RpcClient;
}
```

### RpcServer

```rust
pub struct RpcServer { /* ... */ }

impl RpcServer {
    pub fn new(config: RpcConfig) -> Self;
    pub fn from_config(config: &RszeroConfig) -> Self;
    pub fn graceful_timeout(self, timeout: Duration) -> Self;
    pub fn health(&self) -> &Health;
    pub async fn start(self) -> RszeroResult<()>;
    pub fn config(&self) -> &RpcConfig;
}
```

---

## config

配置管理，支持 YAML/TOML/环境变量/.env 文件。

### RszeroConfig

```rust
pub struct RszeroConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub log: LogConfig,
    pub cache: CacheConfig,
    pub store: StoreConfig,
    pub rpc: RpcConfig,
    pub queue: QueueConfig,
    pub discovery: DiscoveryConfig,
}
```

### 函数

```rust
pub fn load_config(path: &str) -> RszeroResult<RszeroConfig>;
pub fn load_config_with_env(path: &str, env: &str) -> RszeroResult<RszeroConfig>;
```

### ConfigWatcher

```rust
pub struct ConfigWatcher { /* ... */ }

impl ConfigWatcher {
    pub fn start(path: &str, interval: Duration) -> RszeroResult<Self>;
    pub fn get(&self) -> RszeroConfig;
    pub fn subscribe(&self) -> watch::Receiver<RszeroConfig>;
}
```

---

## log

结构化日志，基于 tracing。

```rust
pub fn init(config: &LogConfig);
pub fn info(msg: &str);
pub fn warn(msg: &str);
pub fn error(msg: &str);
pub fn debug(msg: &str);
pub fn trace_span(name: &str) -> tracing::Span;
```

---

## cache

### Redis Cache

```rust
pub struct Cache { /* ... */ }

impl Cache {
    pub async fn new(config: &CacheConfig) -> RszeroResult<Self>;
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> RszeroResult<Option<T>>;
    pub async fn set(&self, key: &str, value: &impl Serialize) -> RszeroResult<()>;
    pub async fn set_ex(&self, key: &str, value: &impl Serialize, ttl: u64) -> RszeroResult<()>;
    pub async fn del(&self, key: &str) -> RszeroResult<()>;
    pub async fn exists(&self, key: &str) -> RszeroResult<bool>;
}
```

### MemCache (内存缓存)

```rust
pub struct Cache<K, V> { /* ... */ }

impl<K: Hash + Eq + Clone, V: Clone> Cache<K, V> {
    pub fn new(capacity: usize) -> Self;
    pub fn get(&self, key: &K) -> Option<V>;
    pub fn set(&self, key: K, value: V);
    pub fn set_with_ttl(&self, key: K, value: V, ttl: Option<Duration>);
    pub fn delete(&self, key: &K) -> bool;
    pub fn contains(&self, key: &K) -> bool;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn clear(&self);
}
```

### DistributedLock

```rust
pub struct DistributedLock { /* ... */ }

impl DistributedLock {
    pub async fn try_acquire(
        cache: Cache,
        resource: &str,
        ttl: Duration,
    ) -> RszeroResult<Option<Self>>;
    pub async fn release(self) -> RszeroResult<()>;
    pub fn resource(&self) -> &str;
}

pub async fn with_lock<T, F, Fut>(
    cache: Cache,
    resource: &str,
    ttl: Duration,
    f: F,
) -> RszeroResult<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>;
```

### Cache-Aside

```rust
pub async fn cache_aside<K, V, F, Fut, E>(
    cache: &Cache<K, V>,
    key: K,
    ttl: Duration,
    loader: F,
) -> Result<V, E>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone + Serialize + DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<V, E>>;
```

---

## queue

消息队列。

```rust
pub struct Queue { /* ... */ }

impl Queue {
    pub fn new(config: QueueConfig) -> Self;
    pub async fn connect(&self) -> RszeroResult<()>;
    pub async fn push(&self, topic: &str, payload: &impl Serialize) -> RszeroResult<()>;
    pub async fn pull(&self, topic: &str) -> RszeroResult<Option<String>>;
    pub async fn pending_count(&self, topic: &str) -> RszeroResult<usize>;
    pub async fn ack(&self, topic: &str) -> RszeroResult<()>;
    pub async fn nack(&self, topic: &str) -> RszeroResult<()>;
    pub fn config(&self) -> &QueueConfig;
}
```

---

## store

数据库连接和迁移。

```rust
pub struct Store { /* ... */ }

impl Store {
    pub async fn new(config: &StoreConfig) -> RszeroResult<Self>;
    pub async fn connect(dsn: &str) -> RszeroResult<Self>;
    pub fn conn(&self) -> &DatabaseConnection;
    pub async fn close(self) -> RszeroResult<()>;
    pub async fn ping(&self) -> RszeroResult<()>;
}

pub struct PoolStats {
    pub size: u32,
    pub available: u32,
    pub acquired: u32,
}
```

### Migrator

```rust
pub struct Migrator { /* ... */ }

impl Migrator {
    pub fn new(store: Store, migration_dir: &str) -> Self;
    pub async fn pending(&self) -> RszeroResult<Vec<String>>;
    pub async fn version(&self) -> RszeroResult<Option<String>>;
    pub fn migration_dir(&self) -> &str;
}
```

---

## limit

限流。

```rust
pub fn rate_limiter() -> GovernorLayer<'static, SmartIpKeyExtractor, NoOpMiddleware>;
pub fn custom_rate_limiter(
    per_second: u64,
    burst_size: u32,
) -> GovernorLayer<'static, SmartIpKeyExtractor, NoOpMiddleware>;
```

---

## breaker

熔断器。

```rust
pub struct CircuitBreaker { /* ... */ }

impl CircuitBreaker {
    pub fn new(failure_threshold: u32) -> Self;
    pub fn is_open(&self) -> bool;
    pub fn record_success(&self);
    pub fn record_failure(&self);
    pub fn reset(&self);
    pub async fn execute<F, T, E>(&self, f: F) -> Result<T, RszeroError>
    where
        F: Future<Output = Result<T, E>>,
        E: Display;
}
```

---

## discovery

服务发现。

```rust
pub struct ServiceInstance {
    pub name: String,
    pub addr: String,
    pub metadata: HashMap<String, String>,
}

pub struct ServiceDiscovery { /* ... */ }

impl ServiceDiscovery {
    pub fn new(config: DiscoveryConfig) -> Self;
    pub fn from_etcd(hosts: Vec<String>) -> Self;
    pub async fn connect(&self) -> RszeroResult<()>;
    pub async fn register(&self, service: &str, addr: &str) -> RszeroResult<()>;
    pub async fn deregister(&self, service: &str, addr: &str) -> RszeroResult<()>;
    pub async fn discover(&self, service: &str) -> RszeroResult<Vec<ServiceInstance>>;
    pub fn kind(&self) -> &str;
}
```

---

## retry

重试机制。

```rust
pub struct RetryPolicy { /* ... */ }

impl RetryPolicy {
    pub fn new() -> Self;
    pub fn max_retries(self, retries: u32) -> Self;
    pub fn initial_delay(self, delay: Duration) -> Self;
    pub fn max_delay(self, delay: Duration) -> Self;
    pub fn multiplier(self, multiplier: f64) -> Self;
    pub fn jitter(self, jitter: bool) -> Self;
    pub fn get_max_retries(&self) -> u32;
    pub fn get_initial_delay(&self) -> Duration;
    pub fn get_max_delay(&self) -> Duration;
}

pub async fn with_retry<F, Fut, T, E>(
    policy: &RetryPolicy,
    operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Display;
```

---

## shedder

负载脱落。

```rust
pub struct AdaptiveShedder { /* ... */ }

impl AdaptiveShedder {
    pub fn new(latency_threshold_ms: u64) -> Self;
    pub fn record_latency(&self, latency_ms: u64);
    pub fn should_reject(&self) -> bool;
    pub fn activate(&self);
    pub fn deactivate(&self);
    pub fn is_active(&self) -> bool;
    pub fn reject_probability(&self) -> u64;
}
```

---

## timeout

超时控制。

```rust
pub trait TimeoutExt: Future + Sized {
    async fn timeout(self, duration: Duration) -> Option<Self::Output>;
    async fn timeout_err<E>(self, duration: Duration, err: E) -> Result<Self::Output, E>;
}
```

---

## health

健康检查。

```rust
pub struct Health { /* ... */ }

impl Health {
    pub fn new() -> Self;
    pub fn set_not_ready(&self);
    pub fn set_ready(&self);
    pub fn is_ready(&self) -> bool;
}
```

---

## metrics

Prometheus 指标。

```rust
pub struct Metrics { /* ... */ }

impl Metrics {
    pub fn new(service_name: &str) -> Self;
    pub fn with_label(&self, key: &str, value: &str);
    pub fn start_request(&self) -> RequestGuard;
    pub fn record_success(&self);
    pub fn record_error(&self);
    pub fn export_prometheus(&self) -> String;
}

pub struct RequestGuard { /* drops to record duration */ }
```

---

## openapi

OpenAPI 文档生成。

```rust
pub struct OpenApiSpec { /* ... */ }

impl OpenApiSpec {
    pub fn new(title: &str, version: &str) -> Self;
    pub fn description(self, desc: &str) -> Self;
    pub fn path(self, path: &str, method: &str, operation: ApiOperation) -> Self;
    pub fn security_scheme(self, name: &str, scheme: SecurityScheme) -> Self;
    pub fn to_json(&self) -> Result<String, serde_json::Error>;
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error>;
}

pub struct ApiOperation { /* ... */ }
pub struct Parameter { /* ... */ }
pub struct SecurityScheme { /* ... */ }
```

---

## middleware

中间件集合。

```rust
// JWT
pub struct JwtMiddleware { /* ... */ }
pub struct Claims { pub sub: String, pub exp: usize }

// Request ID
pub const REQUEST_ID_HEADER: &str = "X-Request-Id";
pub async fn request_id_middleware(req: Request, next: Next) -> Response;
pub fn get_request_id(res: &Response) -> Option<&str>;

// Trace
pub const TRACE_ID_HEADER: &str = "X-Trace-Id";
pub const SPAN_ID_HEADER: &str = "X-Span-Id";
pub const PARENT_SPAN_ID_HEADER: &str = "X-Parent-Span-Id";
pub const SAMPLED_HEADER: &str = "X-Sampled";
pub async fn trace_middleware(req: Request, next: Next) -> Response;

// Validation
pub struct ValidationRules { /* ... */ }
pub fn validation_middleware(rules: ValidationRules) -> impl Fn(Request, Next) -> ...;

// Body Size Limit
pub fn body_size_limit(max_bytes: usize) -> impl Fn(Request, Next) -> ...;
```

---

## concurrent

并发工具。

### MapReduce

```rust
pub enum MapResult<T> { Ok(T), Discard, Err(String) }

pub async fn map_reduce<I, O, R, M, D>(
    items: Vec<I>,
    mapper: M,
    reducer: D,
) -> R;

pub async fn map_reduce_with_concurrency<I, O, R, M, D>(
    items: Vec<I>,
    mapper: M,
    reducer: D,
    concurrency: usize,
) -> R;

pub async fn run_all<F, T>(futures: Vec<F>) -> Vec<T>;
pub async fn run_all_err<F, T, E>(futures: Vec<F>) -> Result<Vec<T>, E>;
```

### FxStream

```rust
pub struct FxStream<T> { /* ... */ }

impl<T> FxStream<T> {
    pub fn from(items: Vec<T>) -> Self;
    pub fn map<U, F>(self, f: F) -> FxStream<U>;
    pub fn filter<F>(self, f: F) -> FxStream<T>;
    pub fn head(self, n: usize) -> FxStream<T>;
    pub fn walk<F>(self, f: F) -> FxStream<T>;
    pub fn done(self) -> Vec<T>;
    pub fn reduce<U, F>(self, f: F) -> Option<U>;
}

pub fn from<T>(items: Vec<T>) -> FxStream<T>;
```

---

## error

统一错误处理。

```rust
#[derive(Error)]
pub enum RszeroError {
    Config(String),
    Database(String),
    Cache(String),
    Rpc(String),
    Http { code: u16, msg: String },
    Auth(String),
    RateLimit,
    CircuitBreaker,
    NotFound(String),
    Discovery(String),
    Queue(String),
    Serialization(serde_json::Error),
    Internal(String),
}

impl RszeroError {
    pub fn config(msg: impl Into<String>) -> Self;
    pub fn database(msg: impl Into<String>) -> Self;
    pub fn cache(msg: impl Into<String>) -> Self;
    pub fn rpc(msg: impl Into<String>) -> Self;
    pub fn http(code: u16, msg: impl Into<String>) -> Self;
    pub fn auth(msg: impl Into<String>) -> Self;
    pub fn discovery(msg: impl Into<String>) -> Self;
    pub fn queue(msg: impl Into<String>) -> Self;
    pub fn not_found(msg: impl Into<String>) -> Self;
    pub fn internal(msg: impl Into<String>) -> Self;
    pub fn status_code(&self) -> u16;
    pub fn code(&self) -> i32;
}

pub struct ErrorResponse {
    pub code: i32,
    pub msg: String,
}

pub type RszeroResult<T> = Result<T, RszeroError>;
```

---

## utils

工具函数。

```rust
pub fn generate_id() -> String;           // UUID v4
pub fn generate_short_id() -> String;     // 12-char short ID
pub fn now_timestamp() -> i64;            // Unix timestamp
pub fn now_iso8601() -> String;           // ISO 8601 timestamp
```
