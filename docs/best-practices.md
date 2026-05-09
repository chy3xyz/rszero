# rszero 最佳实践

> 本文档总结 rszero 框架的生产级开发规范，融合云原生微服务最佳实践与 Rust 生态的工程特性。

---

## 目录

- [1. 项目结构](#1-项目结构)
- [2. REST API 开发](#2-rest-api-开发)
- [3. RPC 服务开发](#3-rpc-服务开发)
- [4. 配置管理](#4-配置管理)
- [5. 日志规范](#5-日志规范)
- [6. 错误处理](#6-错误处理)
- [7. 缓存策略](#7-缓存策略)
- [8. 数据库操作](#8-数据库操作)
- [9. 服务治理](#9-服务治理)
- [10. 中间件使用](#10-中间件使用)
- [11. 分布式追踪](#11-分布式追踪)
- [12. 并发编程](#12-并发编程)
- [13. 部署方案](#13-部署方案)
- [14. 测试策略](#14-测试策略)

---

## 1. 项目结构

### 1.1 框架仓库结构

```
rszero/                          # 框架核心
├── rszero/                      # 主 crate — 用户一键引入
│   └── src/
│       ├── lib.rs               # 统一导出 (prelude)
│       ├── rest/                # Axum 封装
│       ├── rpc/                 # Volo 封装
│       ├── config/              # 配置管理
│       ├── log/                 # 结构化日志
│       ├── cache/               # Redis 缓存
│       ├── queue/               # 消息队列
│       ├── store/               # 数据库/ORM
│       ├── limit/               # 全局限流
│       ├── breaker/             # 熔断降级
│       ├── discovery/           # 服务发现
│       ├── middleware/          # 通用中间件
│       ├── trace/               # 链路追踪
│       └── error/               # 全局错误处理
├── rszeroctl/                   # 代码生成脚手架
└── examples/                    # 官方示例
```

### 1.2 业务项目结构

```
your-project/
├── etc/                         # 配置文件
│   ├── api.yaml
│   └── user-rpc.yaml
├── api/                         # API 网关层
│   ├── src/main.rs
│   └── Cargo.toml
├── rpc/                         # RPC 微服务层
│   ├── user/
│   │   ├── src/main.rs
│   │   └── Cargo.toml
│   └── order/
├── idl/                         # 公共 IDL 文件
├── common/                      # 公共类型和工具
└── deploy/                      # 部署配置
```

**最佳实践**：
- API 网关和 RPC 服务分离部署，独立扩缩容
- 每个微服务独立 `Cargo.toml`，通过 workspace 统一管理
- IDL 文件集中管理，避免循环依赖

---

## 2. REST API 开发

### 2.1 路由定义

```rust
use rszero::prelude::*;
use axum::routing::{get, post, put, delete};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = load_config("etc/api.yaml")?;
    log::init(&config.log);

    let server = RszeroServer::from_config(&config)
        .route("/v1/users", get(list_users).post(create_user))
        .route("/v1/users/:id", get(get_user).put(update_user).delete(delete_user))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(middleware::from_fn(trace_middleware));

    server.start().await?;
    Ok(())
}
```

### 2.2 Handler 规范

```rust
use serde::{Deserialize, Serialize};
use axum::{Json, extract::{State, Path, Query}};

#[derive(Debug, Deserialize)]
struct CreateUserReq {
    name: String,
    email: String,
}

#[derive(Debug, Serialize)]
struct UserResp {
    id: String,
    name: String,
    email: String,
}

async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateUserReq>,
) -> impl IntoResponse {
    // 1. 参数校验
    if req.name.is_empty() {
        return JsonResponse::<UserResp>::error(400, "name is required");
    }

    // 2. 业务逻辑
    let user = UserResp {
        id: generate_short_id(),
        name: req.name,
        email: req.email,
    };

    // 3. 返回响应
    JsonResponse::ok(user)
}
```

**最佳实践**：
- Handler 只负责参数提取和响应返回，业务逻辑下沉到 Service 层
- 使用 `JsonResponse<T>` 统一响应格式，对齐 go-zero 的 `httpx.OkJson`
- 参数校验在 Handler 层完成，非法请求尽早返回
- 使用 `impl IntoResponse` 作为返回类型，避免泛型约束

### 2.3 响应格式

```json
// 成功响应
{
  "code": 0,
  "msg": "ok",
  "data": { "id": "abc123", "name": "John" }
}

// 错误响应
{
  "code": 404,
  "msg": "User not found"
}
```

---

## 3. RPC 服务开发

### 3.1 SVC 模式（Service Context）

```rust
pub struct UserSvc {
    pub config: RszeroConfig,
    pub cache: MemCache<String, User>,
    pub db: Store,
    pub breaker: CircuitBreaker,
}

impl UserSvc {
    pub fn new(config: RszeroConfig) -> Self {
        Self {
            cache: MemCache::new(500),
            db: Store::new(&config.store).await.unwrap(),
            breaker: CircuitBreaker::new(5),
            config,
        }
    }
}
```

### 3.2 Logic 层

```rust
pub struct GetUserLogic {
    svc: Arc<UserSvc>,
}

impl GetUserLogic {
    pub fn new(svc: Arc<UserSvc>) -> Self {
        Self { svc }
    }

    pub async fn execute(&self, user_id: &str) -> RszeroResult<User> {
        // 1. 查缓存
        let cache_key = format!("user:{}", user_id);
        if let Some(user) = self.svc.cache.get(&cache_key) {
            return Ok(user);
        }

        // 2. 熔断保护下游调用
        let user = self.svc.breaker.execute(async {
            // 查数据库
            self.query_user(user_id).await
        }).await?;

        // 3. 回写缓存
        self.svc.cache.set_with_ttl(&cache_key, user.clone(), Some(Duration::from_secs(300)));

        Ok(user)
    }
}
```

**最佳实践**：
- 严格遵循 SVC → Logic 分层，禁止 Handler 直接访问数据库
- 所有外部调用（DB、Cache、RPC）必须经过 CircuitBreaker
- Logic 层保持无状态，通过 SVC 获取依赖

---

## 4. 配置管理

### 4.1 YAML 配置

```yaml
Name: user-api
Host: 0.0.0.0
Port: 8080
Log:
  Level: info
  Format: json
  Output: stdout
Cache:
  Host: 127.0.0.1
  Port: 6379
  Db: 0
  PoolSize: 10
Store:
  Dsn: "postgres://user:pass@localhost:5432/db"
  MaxConnections: 10
  MinConnections: 2
Discovery:
  Kind: etcd
  Endpoints:
    - 127.0.0.1:2379
```

### 4.2 环境变量覆盖

```bash
# .env 文件
RSZERO_LOG_LEVEL=debug
RSZERO_CACHE_HOST=redis-cluster.internal
```

### 4.3 配置热重载

```rust
use rszero::prelude::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let watcher = ConfigWatcher::start("etc/api.yaml", Duration::from_secs(10))?;

    // 初始配置
    let config = watcher.get();
    log::init(&config.log);

    // 监听配置变更
    let mut rx = watcher.subscribe();
    tokio::spawn(async move {
        while let Ok(new_config) = rx.changed().await {
            log::info("configuration reloaded");
            // 热更新逻辑
        }
    });

    Ok(())
}
```

**最佳实践**：
- 敏感信息（密码、密钥）通过环境变量注入，不写入 YAML
- 生产环境使用配置中心（Nacos/Apollo），配合 `ConfigWatcher` 热重载
- 配置变更时记录审计日志

---

## 5. 日志规范

### 5.1 日志级别

| 级别 | 使用场景 |
|------|----------|
| `ERROR` | 系统异常、数据不一致、外部依赖不可用 |
| `WARN` | 降级触发、熔断开启、重试超限 |
| `INFO` | 服务启停、关键业务操作、配置加载 |
| `DEBUG` | 请求参数、SQL 语句、缓存命中/未命中 |

### 5.2 结构化日志

```rust
// 推荐：结构化字段
tracing::info!(
    user_id = %user_id,
    duration_ms = elapsed.as_millis(),
    cache_hit = true,
    "user fetched"
);

// 不推荐：字符串拼接
log::info(&format!("user {} fetched in {}ms", user_id, elapsed.as_millis()));
```

### 5.3 日志输出

```yaml
Log:
  Level: info
  Format: json        # json 用于生产，text 用于开发
  Output: /var/log    # 文件输出，自动按天轮转
```

**最佳实践**：
- 生产环境使用 JSON 格式，便于日志采集系统解析
- 日志中包含 `trace_id` 和 `request_id`，支持全链路追踪
- 禁止在日志中输出敏感信息（密码、Token、身份证号）

---

## 6. 错误处理

### 6.1 统一错误类型

```rust
use rszero::prelude::*;

// 业务错误
pub enum UserServiceError {
    UserNotFound(String),
    EmailAlreadyExists(String),
}

// 转换为 RszeroError
impl From<UserServiceError> for RszeroError {
    fn from(e: UserServiceError) -> Self {
        match e {
            UserServiceError::UserNotFound(id) => RszeroError::not_found(id),
            UserServiceError::EmailAlreadyExists(email) => RszeroError::http(409, email),
        }
    }
}
```

### 6.2 错误响应

```rust
// Handler 层
async fn handler() -> impl IntoResponse {
    match do_something().await {
        Ok(data) => JsonResponse::ok(data),
        Err(e) => {
            tracing::error!(error = %e, "handler failed");
            JsonResponse::<()>::error(e.code(), e.to_string())
        }
    }
}
```

**最佳实践**：
- 业务错误定义在 `common` 模块，各服务共享
- 所有错误最终转换为 `RszeroError`，保证错误码统一
- 错误日志必须包含上下文信息（user_id、request_id）

---

## 7. 缓存策略

### 7.1 Cache-Aside 模式

```rust
use rszero::prelude::*;

async fn get_user(cache: &MemCache<String, User>, user_id: &str) -> RszeroResult<User> {
    cache_aside(
        cache,
        format!("user:{}", user_id),
        Duration::from_secs(300),
        || async { query_user_from_db(user_id).await },
    ).await
}
```

### 7.2 缓存穿透防护

```rust
// 空值缓存，TTL 较短
if let Some(None) = cache.get::<Option<User>>(&key) {
    return Ok(None); // 缓存了空值
}

let user = query_db(id).await?;
let ttl = if user.is_some() { 300 } else { 10 }; // 空值 10s，有效值 5min
cache.set_with_ttl(&key, &user, Some(Duration::from_secs(ttl)));
```

### 7.3 分布式锁

```rust
use rszero::cache::{DistributedLock, with_lock};

// 方式一：手动管理
let lock = DistributedLock::try_acquire(cache_config, "order:create", Duration::from_secs(5))
    .await?
    .ok_or("lock busy")?;
// ... 业务逻辑
lock.release().await?;

// 方式二：自动管理
with_lock(cache_config, "order:create", Duration::from_secs(5), || async {
    // 业务逻辑，执行完自动释放
}).await?;
```

**最佳实践**：
- 缓存 Key 格式：`{service}:{entity}:{id}`，如 `user:profile:12345`
- 缓存 TTL 设置：热点数据 5min，非热点 1min，空值 10s
- 分布式锁必须设置 TTL，防止死锁
- 使用 `with_lock` 自动释放，避免忘记释放导致死锁

---

## 8. 数据库操作

### 8.1 连接池配置

```yaml
Store:
  Dsn: "postgres://user:pass@localhost:5432/db"
  MaxConnections: 10    # 根据 CPU 核心数调整
  MinConnections: 2     # 保持最小连接数
```

### 8.2 迁移管理

```rust
use rszero::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let store = Store::new(&config.store).await?;
    let migrator = Migrator::new(store, "migrations");

    // 查看待执行的迁移
    let pending = migrator.pending().await?;
    log::info(&format!("{} migrations pending", pending.len()));

    // 执行迁移
    migrator.run().await?;

    // 查看当前版本
    let version = migrator.version().await?;
    log::info(&format!("current version: {:?}", version));

    Ok(())
}
```

### 8.3 事务使用

```rust
use sea_orm::{TransactionTrait, TransactionError};

async fn transfer_money(db: &DatabaseConnection, from: i32, to: i32, amount: f64) -> RszeroResult<()> {
    let txn = db.begin().await?;

    // 扣款
    sea_orm::query::Query::update()
        .table(Users::Entity)
        .col_expr(Users::Column::Balance, Expr::col(Users::Column::Balance).sub(amount))
        .filter(Users::Column::Id.eq(from))
        .exec(&txn)
        .await?;

    // 收款
    sea_orm::query::Query::update()
        .table(Users::Entity)
        .col_expr(Users::Column::Balance, Expr::col(Users::Column::Balance).add(amount))
        .filter(Users::Column::Id.eq(to))
        .exec(&txn)
        .await?;

    txn.commit().await?;
    Ok(())
}
```

**最佳实践**：
- 连接池大小 = CPU 核心数 × 2 + 磁盘数
- 长事务拆分，避免持有连接过久
- 迁移文件按版本号命名：`001_create_users.sql`
- 生产环境迁移使用蓝绿发布

---

## 9. 服务治理

### 9.1 限流

```rust
use rszero::prelude::*;

// 默认限流：10 req/s，burst 30
let app = axum::Router::new()
    .route("/v1/users", get(list_users))
    .layer(rate_limiter());

// 自定义限流
let app = axum::Router::new()
    .route("/v1/login", post(login))
    .layer(custom_rate_limiter(5, 10)); // 5 req/s, burst 10
```

### 9.2 熔断

```rust
use rszero::prelude::*;

let breaker = CircuitBreaker::new(5); // 连续 5 次失败后熔断

// 使用熔断器执行远程调用
let result = breaker.execute(async {
    call_external_service().await
}).await?;
```

### 9.3 负载脱落（Load Shedding）

```rust
use rszero::prelude::*;

let shedder = AdaptiveShedder::new(200); // 延迟超过 200ms 开始脱落

async fn handler(State(state): State<Arc<AppState>>, req: Request) -> Response {
    if state.shedder.should_reject() {
        return JsonResponse::error(503, "service overloaded");
    }
    // 处理请求
}
```

### 9.4 超时控制

```rust
use rszero::prelude::*;

// 单个操作超时
let result = some_async_operation()
    .timeout(Duration::from_secs(3))
    .await;

match result {
    Some(value) => { /* 成功 */ }
    None => { /* 超时 */ }
}
```

### 9.5 重试机制

```rust
use rszero::prelude::*;

let policy = RetryPolicy::new()
    .max_retries(3)
    .initial_delay(Duration::from_millis(100))
    .max_delay(Duration::from_secs(5))
    .multiplier(2.0)
    .jitter(true); // 启用抖动，避免惊群效应

let result = with_retry(&policy, || async {
    call_unreliable_service().await
}).await?;
```

**最佳实践**：
- 限流放在最外层，最先拦截过量请求
- 熔断保护所有外部调用（DB、Cache、RPC、HTTP）
- 负载脱落保护系统不被压垮
- 重试仅用于幂等操作，必须配合指数退避和抖动

---

## 10. 中间件使用

### 10.1 中间件链

```rust
let app = axum::Router::new()
    .route("/v1/users", get(list_users))
    // 顺序很重要：从外到内执行
    .layer(middleware::from_fn(shedder_middleware))       // 1. 负载脱落
    .layer(middleware::from_fn(request_id_middleware))    // 2. 请求 ID
    .layer(middleware::from_fn(trace_middleware))         // 3. 链路追踪
    .layer(middleware::from_fn(body_size_limit(10 * 1024 * 1024))) // 4. 体大小限制
    .layer(rate_limiter());                                // 5. 限流
```

### 10.2 JWT 认证

```rust
let jwt = JwtMiddleware::new("your-secret-key");

// 生成 Token
let token = jwt.generate_token("user-123", 86400)?; // 24h 过期

// 验证 Token
let claims = jwt.verify_token(&token)?;
assert_eq!(claims.sub, "user-123");
```

### 10.3 请求验证

```rust
let rules = ValidationRules::new()
    .max_body_size(10 * 1024 * 1024) // 10MB
    .required_headers(vec!["X-Api-Key".into()]);

let app = axum::Router::new()
    .route("/v1/data", post(create_data))
    .layer(middleware::from_fn(validation_middleware(rules)));
```

**最佳实践**：
- 中间件顺序：脱落 → 请求ID → 追踪 → 验证 → 限流 → 业务
- JWT Secret 通过环境变量注入，定期轮换
- 请求验证放在业务逻辑之前，尽早拒绝非法请求

---

## 11. 分布式追踪

### 11.1 初始化

```rust
use rszero::prelude::*;

// 初始化 Jaeger 追踪
trace::init_tracer("user-service")?;

// 获取 Tracer
let tracer = trace::get_tracer("user-service");
```

### 11.2 跨服务传播

```rust
// 自动传播 trace_id 和 span_id
let app = axum::Router::new()
    .route("/v1/users", get(list_users))
    .layer(middleware::from_fn(trace_middleware));

// 下游服务自动接收 trace headers
// X-Trace-Id, X-Span-Id, X-Parent-Span-Id, X-Sampled
```

### 11.3 自定义 Span

```rust
use tracing::instrument;

#[instrument(name = "fetch_user", fields(user_id = %id))]
async fn fetch_user(id: &str) -> RszeroResult<User> {
    // 自动记录 span
    tracing::debug!("querying database");
    // ...
}
```

**最佳实践**：
- 所有 HTTP/RPC 入口必须经过 `trace_middleware`
- Span 命名格式：`{service}.{operation}`，如 `user.get_user`
- 敏感字段不记录到 Span 中

---

## 12. 并发编程

### 12.1 MapReduce

```rust
use rszero::prelude::*;

let items = vec![1, 2, 3, 4, 5];
let sum: i64 = map_reduce(
    items,
    |item| Box::pin(async move { MapResult::Ok((item * 2) as i64) }),
    |results| results.into_iter().sum::<i64>(),
).await;
```

### 12.2 函数式流

```rust
use rszero::prelude::*;

let result = fx::from(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
    .filter(|x| x % 2 == 0)      // [2, 4, 6, 8, 10]
    .map(|x| x * 10)              // [20, 40, 60, 80, 100]
    .head(3)                      // [20, 40, 60]
    .done();
```

**最佳实践**：
- MapReduce 适用于数据并行处理场景
- 函数式流适用于数据管道场景
- 注意控制并发度，避免资源耗尽

---

## 13. 部署方案

### 13.1 Docker

```dockerfile
# 多阶段构建
FROM rust:1.75-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY . .
RUN cargo build --release --bin bookstore-api

FROM alpine:3.19
RUN apk --no-cache add ca-certificates
WORKDIR /root/
COPY --from=builder /app/target/release/bookstore-api .
EXPOSE 8080
CMD ["./bookstore-api"]
```

### 13.2 Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: bookstore-api
spec:
  replicas: 3
  selector:
    matchLabels:
      app: bookstore-api
  template:
    metadata:
      labels:
        app: bookstore-api
    spec:
      containers:
        - name: api
          image: bookstore-api:latest
          ports:
            - containerPort: 8080
          resources:
            requests:
              memory: "64Mi"
              cpu: "100m"
            limits:
              memory: "256Mi"
              cpu: "500m"
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
          readinessProbe:
            httpGet:
              path: /health
              port: 8080
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: bookstore-api
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: bookstore-api
  minReplicas: 3
  maxReplicas: 10
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
```

### 13.3 健康检查

```rust
// 自动注册 /health 端点
let health = Health::new();

// 服务启动
health.set_ready();

// 服务关闭（优雅停机）
health.set_not_ready();
```

**最佳实践**：
- 使用多阶段构建，最终镜像 < 20MB
- 配置 liveness + readiness 探针
- 配置 HPA 自动扩缩容
- 配置资源限制，防止 OOM

---

## 14. 测试策略

### 14.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_user() {
        let svc = Arc::new(UserSvc::new(RszeroConfig::default()));
        let logic = CreateUserLogic::new(svc);

        let user = logic.execute(CreateUserReq {
            name: "John".into(),
            email: "john@example.com".into(),
        }).await.unwrap();

        assert_eq!(user.name, "John");
    }
}
```

### 14.2 集成测试

```rust
// tests/integration_test.rs
use rszero::prelude::*;

#[tokio::test]
async fn test_full_request_flow() {
    // 1. 启动服务
    // 2. 发送请求
    // 3. 验证响应
    // 4. 清理数据
}
```

### 14.3 基准测试

```bash
cargo bench
```

**最佳实践**：
- 单元测试覆盖核心业务逻辑
- 集成测试覆盖完整请求链路
- 基准测试监控性能回归
- 测试数据隔离，不依赖外部服务

---

## 附录：go-zero → rszero 对照表

| go-zero | rszero | 说明 |
|---------|--------|------|
| `rest.MustNewServer()` | `RszeroServer::new()` | HTTP 服务器 |
| `zrpc.MustNewServer()` | `RpcServer::new()` | RPC 服务器 |
| `conf.MustLoad()` | `load_config()` | 配置加载 |
| `logx.Info()` | `log::info()` | 日志 |
| `httpx.OkJson()` | `JsonResponse::ok()` | 响应 |
| `httpx.Error()` | `JsonResponse::error()` | 错误响应 |
| `cache.New()` | `Cache::new()` | Redis 缓存 |
| `sqlc.New()` | `Store::new()` | 数据库 |
| `limit.NewPeriodLimit()` | `rate_limiter()` | 限流 |
| `breaker.New()` | `CircuitBreaker::new()` | 熔断 |
| `discov.New()` | `ServiceDiscovery::new()` | 服务发现 |
| `mr.MapReduce()` | `map_reduce()` | 并发处理 |
| `goctl` | `rszeroctl` | 代码生成 |
