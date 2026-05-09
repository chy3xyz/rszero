# Getting Started / 入门教程

> Build your first rszero microservice in 5 minutes.  
> 从零开始，5 分钟构建你的第一个 rszero 微服务。

---

## English

### Prerequisites

- Rust 1.75+ ([install](https://www.rust-lang.org/tools/install))
- Basic knowledge of Rust async programming

### Step 1: Install rszero

```bash
# Clone the repository
git clone https://github.com/chy3xyz/rszero.git
cd rszero

# Build the framework
cargo build --release
```

### Step 2: Create a Project

**Option A: Manual**

```bash
mkdir my-service && cd my-service
cargo init --name my-service
```

Add to `Cargo.toml`:
```toml
[dependencies]
rszero = "0.1"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
axum = { version = "0.7", features = ["full"] }
```

**Option B: Using the CLI scaffold (Recommended)**

```bash
# Install the CLI
cargo install --path rszeroctl

# Generate project
rszeroctl new my-service
cd my-service
```

### Step 3: Write Your First Handler

Edit `src/main.rs`:

```rust
use rszero::prelude::*;
use serde::{Deserialize, Serialize};
use axum::routing::get;

#[derive(Debug, Deserialize)]
struct HelloReq {
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct HelloResp {
    message: String,
    timestamp: String,
}

async fn hello(
    axum::extract::Query(req): axum::extract::Query<HelloReq>,
) -> impl axum::response::IntoResponse {
    let name = req.name.unwrap_or_else(|| "World".into());
    JsonResponse::ok(HelloResp {
        message: format!("Hello, {}!", name),
        timestamp: now_iso8601(),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = rszero::config::RszeroConfig::default();
    log::init(&config.log);

    let server = RszeroServer::new("0.0.0.0", 8080)
        .route("/hello", get(hello))
        .layer(axum::middleware::from_fn(rszero::middleware::request_id_middleware));

    server.start().await?;
    Ok(())
}
```

### Step 4: Run the Service

```bash
cargo run
```

Output:
```
2026-04-04T00:00:00Z  INFO rszero: Server listening on http://0.0.0.0:8080
```

### Step 5: Test the Endpoint

```bash
# Basic request
curl http://localhost:8080/hello
# {"code":0,"msg":"ok","data":{"message":"Hello, World!","timestamp":"..."}}

# With parameter
curl "http://localhost:8080/hello?name=Rust"
# {"code":0,"msg":"ok","data":{"message":"Hello, Rust!","timestamp":"..."}}
```

### Step 6: Add More Capabilities

**Rate Limiting**
```rust
let server = RszeroServer::new("0.0.0.0", 8080)
    .route("/hello", get(hello))
    .layer(rate_limiter()); // 10 req/s, burst 30
```

**Caching**
```rust
let cache = MemCache::new(100);
cache.set("greeting".to_string(), "Hello!".to_string());
let greeting = cache.get(&"greeting".to_string());
```

**Circuit Breaker**
```rust
let breaker = CircuitBreaker::new(5);
let result = breaker.execute(async {
    call_external_api().await
}).await?;
```

**Retry with Backoff**
```rust
let policy = RetryPolicy::new()
    .max_retries(3)
    .initial_delay(std::time::Duration::from_millis(100))
    .jitter(true);

let result = with_retry(&policy, || async {
    call_unreliable_service().await
}).await?;
```

### Step 7: Production Deployment

**Configuration (`etc/api.yaml`)**
```yaml
Name: my-service
Host: 0.0.0.0
Port: 8080
Log:
  Level: info
  Format: json
  Output: stdout
```

**Dockerfile**
```dockerfile
FROM rust:1.75-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY . .
RUN cargo build --release

FROM alpine:3.19
RUN apk --no-cache add ca-certificates
WORKDIR /root/
COPY --from=builder /app/target/release/my-service .
EXPOSE 8080
CMD ["./my-service"]
```

### Next Steps

- Read [Best Practices](best-practices.md) for production guidelines
- Explore [Architecture](architecture.md) for system design
- Check the [bookstore example](../examples/bookstore/) for a complete microservice

---

## 中文

### 前置条件

- Rust 1.75+ ([安装指南](https://www.rust-lang.org/tools/install))
- 基本的 Rust 异步编程知识

### 第一步：安装 rszero

```bash
# 克隆仓库
git clone https://github.com/chy3xyz/rszero.git
cd rszero

# 编译框架
cargo build --release
```

### 第二步：创建项目

**方式一：手动创建**

```bash
mkdir my-service && cd my-service
cargo init --name my-service
```

在 `Cargo.toml` 中添加：
```toml
[dependencies]
rszero = "0.1"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
axum = { version = "0.7", features = ["full"] }
```

**方式二：使用脚手架（推荐）**

```bash
# 安装 CLI
cargo install --path rszeroctl

# 创建项目
rszeroctl new my-service
cd my-service
```

### 第三步：编写第一个 Handler

编辑 `src/main.rs`：

```rust
use rszero::prelude::*;
use serde::{Deserialize, Serialize};
use axum::routing::get;

#[derive(Debug, Deserialize)]
struct HelloReq {
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct HelloResp {
    message: String,
    timestamp: String,
}

async fn hello(
    axum::extract::Query(req): axum::extract::Query<HelloReq>,
) -> impl axum::response::IntoResponse {
    let name = req.name.unwrap_or_else(|| "World".into());
    JsonResponse::ok(HelloResp {
        message: format!("Hello, {}!", name),
        timestamp: now_iso8601(),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = rszero::config::RszeroConfig::default();
    log::init(&config.log);

    let server = RszeroServer::new("0.0.0.0", 8080)
        .route("/hello", get(hello))
        .layer(axum::middleware::from_fn(rszero::middleware::request_id_middleware));

    server.start().await?;
    Ok(())
}
```

### 第四步：运行服务

```bash
cargo run
```

输出：
```
2026-04-04T00:00:00Z  INFO rszero: Server listening on http://0.0.0.0:8080
```

### 第五步：测试接口

```bash
# 基本请求
curl http://localhost:8080/hello
# {"code":0,"msg":"ok","data":{"message":"Hello, World!","timestamp":"..."}}

# 带参数
curl "http://localhost:8080/hello?name=Rust"
# {"code":0,"msg":"ok","data":{"message":"Hello, Rust!","timestamp":"..."}}
```

### 第六步：添加更多功能

**限流**
```rust
let server = RszeroServer::new("0.0.0.0", 8080)
    .route("/hello", get(hello))
    .layer(rate_limiter()); // 10 req/s, burst 30
```

**缓存**
```rust
let cache = MemCache::new(100);
cache.set("greeting".to_string(), "Hello!".to_string());
let greeting = cache.get(&"greeting".to_string());
```

**熔断**
```rust
let breaker = CircuitBreaker::new(5);
let result = breaker.execute(async {
    call_external_api().await
}).await?;
```

**重试**
```rust
let policy = RetryPolicy::new()
    .max_retries(3)
    .initial_delay(std::time::Duration::from_millis(100))
    .jitter(true);

let result = with_retry(&policy, || async {
    call_unreliable_service().await
}).await?;
```

### 第七步：生产部署

**配置文件 (`etc/api.yaml`)**
```yaml
Name: my-service
Host: 0.0.0.0
Port: 8080
Log:
  Level: info
  Format: json
  Output: stdout
```

**Dockerfile**
```dockerfile
FROM rust:1.75-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY . .
RUN cargo build --release

FROM alpine:3.19
RUN apk --no-cache add ca-certificates
WORKDIR /root/
COPY --from=builder /app/target/release/my-service .
EXPOSE 8080
CMD ["./my-service"]
```

### 下一步

- 阅读 [最佳实践](best-practices.md) 了解生产级开发规范
- 查看 [架构设计](architecture.md) 了解系统设计
- 参考 [bookstore 示例](../examples/bookstore/) 学习完整微服务架构
