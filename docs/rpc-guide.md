# Volo RPC 集成指南

> 本文档详细说明如何在 rszero 中使用 Volo gRPC/Thrift 构建 RPC 微服务。

---

## 概述

rszero 使用 [CloudWeGo Volo](https://github.com/cloudwego/volo) 作为 RPC 传输层，提供与 go-zero zrpc 相似的 RPC 能力。

### 架构

```
┌──────────────────────────────────────────────────────────────┐
│                        API Gateway                           │
│                    (Axum REST 0.7)                           │
└──────────────────────────┬───────────────────────────────────┘
                           │ gRPC / Thrift
┌──────────────────────────▼───────────────────────────────────┐
│                    Volo RPC Layer                            │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────┐ │
│  │ volo-grpc    │  │ volo-thrift  │  │ volo-build (IDL)   │ │
│  │ (Protobuf)   │  │ (Thrift)     │  │ (代码生成)          │ │
│  └──────────────┘  └──────────────┘  └────────────────────┘ │
└──────────────────────────┬───────────────────────────────────┘
                           │
┌──────────────────────────▼───────────────────────────────────┐
│                   rszero 治理层                               │
│  ┌──────────┐ ┌──────┐ ┌───────┐ ┌──────┐ ┌──────────────┐  │
│  │Discovery │ │Break │ │ Limit │ │Retry │ │   Metrics    │  │
│  │ (etcd)   │ │(cb)  │ │(tower)│ │(exp) │ │ (prometheus) │  │
│  └──────────┘ └──────┘ └───────┘ └──────┘ └──────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

---

## 快速开始

### 第一步：定义 IDL

#### Protobuf 方式

创建 `idl/user.proto`：

```protobuf
syntax = "proto3";
package user;

option go_package = "./user";

message GetUserRequest {
  int64 id = 1;
}

message GetUserResponse {
  int64 id = 1;
  string name = 2;
  int32 age = 3;
  string email = 4;
}

service UserService {
  rpc GetUser (GetUserRequest) returns (GetUserResponse);
  rpc CreateUser (CreateUserRequest) returns (GetUserResponse);
}

message CreateUserRequest {
  string name = 1;
  int32 age = 2;
  string email = 3;
}
```

#### Thrift 方式

创建 `idl/user.thrift`：

```thrift
namespace rs user

struct User {
  1: required i64 id,
  2: required string name,
  3: required i32 age,
  4: required string email,
}

struct GetUserRequest {
  1: required i64 id,
}

struct CreateUserRequest {
  1: required string name,
  2: required i32 age,
  3: required string email,
}

service UserService {
  User GetUser(1: GetUserRequest req),
  User CreateUser(1: CreateUserRequest req),
}
```

---

### 第二步：配置 volo-build

创建 `volo-gen/Cargo.toml`：

```toml
[package]
name = "volo-gen"
version = "0.1.0"
edition = "2021"

[dependencies]
volo = "0.12"
volo-grpc = "0.12"
volo-thrift = "0.12"
pilota = "0.11"
pilota-build = "0.11"
```

创建 `volo-gen/src/lib.rs`：

```rust
// Protobuf 方式
volo_build::config_builder()
    .add_service("../idl/user.proto")
    .build()
    .gen()
    .unwrap();

// 或 Thrift 方式
volo_build::config_builder()
    .add_service("../idl/user.thrift")
    .build()
    .gen()
    .unwrap();
```

---

### 第三步：实现服务

```rust
use rszero::prelude::*;
use volo_gen::user::user_service::{UserService, UserServiceServer};

pub struct UserHandler {
    svc: Arc<UserSvc>,
}

#[volo::async_trait]
impl UserService for UserHandler {
    async fn get_user(
        &self,
        req: volo_gen::user::GetUserRequest,
    ) -> Result<volo_gen::user::GetUserResponse, volo_gen::user::GetUserError> {
        // 使用 rszero 的缓存 + 熔断
        let cache_key = format!("user:{}", req.id);
        if let Some(user) = self.svc.cache.get(&cache_key) {
            return Ok(user);
        }

        // 通过熔断器查询数据库
        let user = self.svc.breaker.execute(async {
            self.query_user(req.id).await
        }).await?;

        // 回写缓存
        self.svc.cache.set_with_ttl(&cache_key, &user, Some(Duration::from_secs(300)));

        Ok(user)
    }
}
```

---

### 第四步：启动 RPC 服务器

```rust
use rszero::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 加载配置
    let config = load_config("etc/user-rpc.yaml")?;
    log::init(&config.log);

    // 创建服务上下文
    let svc = Arc::new(UserSvc::new(config.clone()));

    // 创建 RPC 服务器
    let rpc_server = RpcServer::from_config(&config);

    // 注册到 etcd
    let discovery = ServiceDiscovery::from_etcd(vec!["127.0.0.1:2379".into()]);
    discovery.register("user.rpc", &config.rpc.listen_on).await?;

    log::info!("User RPC service ready on {}", config.rpc.listen_on);

    // 启动
    rpc_server.start().await?;
    Ok(())
}
```

---

## 服务发现

### etcd 注册

```yaml
# etc/user-rpc.yaml
Name: user-rpc
ListenOn: 0.0.0.0:8081
Etcd:
  Hosts:
    - 127.0.0.1:2379
  Key: user.rpc
```

```rust
// 注册服务
discovery.register("user.rpc", "127.0.0.1:8081").await?;

// 发现服务
let instances = discovery.discover("user.rpc").await?;
for instance in instances {
    println!("Found: {} at {}", instance.name, instance.addr);
}
```

### 自定义服务发现

```rust
use rszero::discovery::ServiceDiscovery;

// 实现自定义服务发现
let discovery = ServiceDiscovery::new(DiscoveryConfig {
    kind: "custom".into(),
    endpoints: vec!["http://registry.internal:8500".into()],
});
```

---

## 中间件

### 日志中间件

```rust
use volo::Layer;

#[derive(Clone)]
pub struct LogService<S>(S);

impl<Cx, Req, S> volo::Service<Cx, Req> for LogService<S>
where
    Req: Send + 'static,
    S: Send + 'static + volo::Service<Cx, Req> + Sync,
    Cx: Send + 'static,
{
    async fn call(&self, cx: &mut Cx, req: Req) -> Result<S::Response, S::Error> {
        let now = std::time::Instant::now();
        let resp = self.0.call(cx, req).await;
        tracing::info!("RPC call took {}ms", now.elapsed().as_millis());
        resp
    }
}
```

### 认证中间件

```rust
#[derive(Clone)]
pub struct AuthService<S> {
    inner: S,
    jwt: JwtMiddleware,
}

impl<Cx, Req, S> volo::Service<Cx, Req> for AuthService<S>
where
    Req: Send + 'static,
    S: Send + 'static + volo::Service<Cx, Req> + Sync,
    Cx: Send + 'static,
{
    async fn call(&self, cx: &mut Cx, req: Req) -> Result<S::Response, S::Error> {
        // 验证 JWT token
        let token = extract_token(cx)?;
        self.jwt.verify_token(&token)?;
        self.inner.call(cx, req).await
    }
}
```

---

## 客户端调用

### 基本调用

```rust
use rszero::prelude::*;

// 创建客户端
let client = RpcClient::builder(RpcConfig::default())
    .timeout(Duration::from_secs(5))
    .max_retries(3)
    .build();

// 调用远程服务
let result = client.call::<GetUserResponse>("user.GetUser", &req).await?;
```

### 带服务发现的调用

```rust
let discovery = ServiceDiscovery::from_etcd(vec!["127.0.0.1:2379".into()]);

let client = RpcClient::builder(RpcConfig::default())
    .timeout(Duration::from_secs(5))
    .max_retries(3)
    .discovery(discovery)
    .build();
```

---

## 错误处理

```rust
use rszero::prelude::*;

// 业务错误
#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("user not found: {0}")]
    NotFound(i64),
    #[error("email already exists: {0}")]
    EmailExists(String),
}

// 转换为 RszeroError
impl From<UserError> for RszeroError {
    fn from(e: UserError) -> Self {
        match e {
            UserError::NotFound(id) => RszeroError::not_found(format!("user {}", id)),
            UserError::EmailExists(email) => RszeroError::http(409, email),
        }
    }
}
```

---

## 性能优化

### 连接池

```yaml
# etc/user-rpc.yaml
Store:
  MaxConnections: 20    # 根据 CPU 核心数调整
  MinConnections: 5
```

### 批量操作

```rust
use rszero::prelude::*;

// 使用 MapReduce 批量处理
let user_ids = vec![1, 2, 3, 4, 5];
let users: Vec<User> = map_reduce(
    user_ids,
    |id| Box::pin(async move {
        Match client.call::<User>("user.GetUser", &GetUserRequest { id }).await {
            Ok(user) => MapResult::Ok(user),
            Err(_) => MapResult::Discard,
        }
    }),
    |results| results,
).await;
```

---

## 部署

### Docker

```dockerfile
FROM rust:1.75-alpine AS builder
RUN apk add --no-cache musl-dev protobuf-dev
WORKDIR /app
COPY . .
RUN cargo build --release --bin user-rpc

FROM alpine:3.19
RUN apk --no-cache add ca-certificates
WORKDIR /root/
COPY --from=builder /app/target/release/user-rpc .
EXPOSE 8081
CMD ["./user-rpc"]
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: user-rpc
spec:
  replicas: 3
  selector:
    matchLabels:
      app: user-rpc
  template:
    metadata:
      labels:
        app: user-rpc
    spec:
      containers:
        - name: rpc
          image: user-rpc:latest
          ports:
            - containerPort: 8081
          resources:
            requests:
              memory: "128Mi"
              cpu: "200m"
            limits:
              memory: "512Mi"
              cpu: "1000m"
```

---

## 附录：Volo 版本对照

| 组件 | rszero 版本 | Volo 版本 |
|------|------------|-----------|
| volo | 0.12 | 0.12.3 |
| volo-grpc | 0.12 | 0.12.2 |
| volo-thrift | 0.12 | 0.12.4 |
| volo-build | 0.12 | 0.12.2 |

### Volo 0.10 → 0.12 迁移

1. 更新 `Cargo.toml` 中的版本号
2. 运行 `volo migrate` 更新 `volo.yml` 格式
3. 检查错误处理变更（`anyhow::Error` → `volo_thrift::ServerError`）
4. 更新枚举类型引用（i32 newtype 变更）

详见：[Volo 0.10 Release Notes](https://cloudwego.io/blog/2024/04/08/volo-release-0.10.0/)
