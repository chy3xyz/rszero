# rszero 入门教程

> 从零开始，5 分钟构建你的第一个 rszero 微服务。

---

## 前置条件

- Rust 1.75+ ([安装指南](https://www.rust-lang.org/tools/install))
- 基本的 Rust 异步编程知识

---

## 第一步：安装 rszero

```bash
# 克隆仓库
git clone https://github.com/your-org/rszero.git
cd rszero

# 编译框架
cargo build --release
```

---

## 第二步：创建项目

### 方式一：手动创建

```bash
# 创建项目目录
mkdir my-service && cd my-service

# 初始化 Cargo 项目
cargo init --name my-service

# 添加依赖
cat >> Cargo.toml << 'EOF'
[dependencies]
rszero = { path = "../rszero/rszero" }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
axum = { version = "0.7", features = ["full"] }
EOF
```

### 方式二：使用脚手架（推荐）

```bash
# 安装脚手架
cargo install --path rszeroctl

# 创建项目
rszeroctl new my-service
cd my-service
```

---

## 第三步：编写第一个 Handler

编辑 `src/main.rs`：

```rust
use rszero::prelude::*;
use serde::{Deserialize, Serialize};
use axum::routing::get;

// 请求参数
#[derive(Debug, Deserialize)]
struct HelloReq {
    name: Option<String>,
}

// 响应数据
#[derive(Debug, Serialize)]
struct HelloResp {
    message: String,
    timestamp: String,
}

// Handler 函数
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
    // 加载配置
    let config = rszero::config::RszeroConfig::default();

    // 初始化日志
    log::init(&config.log);
    log::info("Starting my-service...");

    // 创建服务器
    let server = RszeroServer::new("0.0.0.0", 8080)
        .route("/hello", get(hello))
        .layer(axum::middleware::from_fn(rszero::middleware::request_id_middleware));

    log::info("Server listening on http://0.0.0.0:8080");

    // 启动服务
    server.start().await?;
    Ok(())
}
```

---

## 第四步：运行服务

```bash
cargo run
```

输出：
```
2026-04-04T00:00:00Z  INFO rszero: Starting my-service...
2026-04-04T00:00:00Z  INFO rszero: Server listening on http://0.0.0.0:8080
```

---

## 第五步：测试接口

```bash
# 基本请求
curl http://localhost:8080/hello
# {"code":0,"msg":"ok","data":{"message":"Hello, World!","timestamp":"2026-04-04T..."}}

# 带参数
curl "http://localhost:8080/hello?name=Rust"
# {"code":0,"msg":"ok","data":{"message":"Hello, Rust!","timestamp":"2026-04-04T..."}}
```

---

## 第六步：添加更多功能

### 6.1 添加限流

```rust
use rszero::prelude::*;

let server = RszeroServer::new("0.0.0.0", 8080)
    .route("/hello", get(hello))
    .layer(rate_limiter()); // 10 req/s, burst 30
```

### 6.2 添加缓存

```rust
use rszero::prelude::*;

let cache = MemCache::new(100);
cache.set("greeting".to_string(), "Hello!".to_string());
let greeting = cache.get(&"greeting".to_string());
```

### 6.3 添加熔断

```rust
use rszero::prelude::*;

let breaker = CircuitBreaker::new(5);
let result = breaker.execute(async {
    call_external_api().await
}).await?;
```

### 6.4 添加重试

```rust
use rszero::prelude::*;

let policy = RetryPolicy::new()
    .max_retries(3)
    .initial_delay(std::time::Duration::from_millis(100))
    .jitter(true);

let result = with_retry(&policy, || async {
    call_unreliable_service().await
}).await?;
```

---

## 第七步：生产部署

### 7.1 配置文件

创建 `etc/api.yaml`：

```yaml
Name: my-service
Host: 0.0.0.0
Port: 8080
Log:
  Level: info
  Format: json
  Output: stdout
```

### 7.2 加载配置

```rust
let config = load_config("etc/api.yaml")?;
log::init(&config.log);

let server = RszeroServer::from_config(&config)
    .route("/hello", get(hello));
```

### 7.3 Docker 部署

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

---

## 下一步

- 阅读 [最佳实践](best-practices.md) 了解生产级开发规范
- 查看 [架构设计](architecture.md) 了解系统设计
- 参考 [bookstore 示例](../../examples/bookstore/) 学习完整微服务架构
