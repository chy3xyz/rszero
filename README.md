# rszero

> **Rust** + **S**ervice + **Zero** — A production-grade microservices framework for the Rust ecosystem.

[![CI](https://github.com/chy3xyz/rszero/actions/workflows/ci.yml/badge.svg)](https://github.com/chy3xyz/rszero/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/rszero.svg)](https://crates.io/crates/rszero)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-blue.svg)](https://www.rust-lang.org)

[English](#english) | [中文](#中文)

---

<a id="english"></a>
## English

### Overview

**rszero** is a cloud-native microservices framework built for the Rust ecosystem, combining modern Rust's memory safety, zero-cost abstractions, and high-performance async runtime with battle-tested distributed systems patterns.

It provides a cohesive, batteries-included toolkit for building REST APIs, RPC services, and full-fledged microservice architectures — without forcing you to manually wire together dozens of disparate crates.

### Why rszero?

| Capability | What You Get |
|-----------|-------------|
| **Memory Safety** | `#![forbid(unsafe_code)]` — entire framework is 100% safe Rust |
| **Type Safety** | Compile-time guarantees for request schemas, API contracts, and database queries |
| **Performance** | Zero-GC, low memory footprint, Tokio-powered async with work-stealing scheduler |
| **Modularity** | Feature-gated compilation — pay only for what you use (`rest`, `rpc`, `cache`, `store`, etc.) |
| **Observability** | Built-in structured logging (tracing), Prometheus metrics, OpenTelemetry tracing, health probes |
| **Resilience** | Circuit breaker, rate limiting, retry with exponential backoff, load shedding, timeout controls |
| **Cloud Native** | etcd service discovery, Docker-ready, Kubernetes health endpoints, graceful shutdown |
| **Developer Experience** | `rszeroctl` codegen scaffold, unified `prelude`, sensible defaults |

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Clients / Frontends                       │
└───────────────────────────────┬─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│              rszero REST Gateway (Axum-powered)                 │
│  Routing │ Auth │ Validation │ Rate Limit │ Breaker │ Metrics   │
└───────────────────────────────┬─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│              Service Discovery (etcd / custom)                   │
└─────────────┬─────────────────────┬──────────────────┬──────────┘
              │                     │                  │
┌─────────────▼──────┐  ┌──────────▼─────┐  ┌────────▼─────────┐
│  rszero RPC User   │  │ rszero RPC     │  │ rszero RPC Pay   │
│  Service (Volo)    │  │ Order Service  │  │ Service (Volo)   │
└─────────────────────┘  └─────────────────┘  └──────────────────┘
       ▲                          ▲                      ▲
       │                          │                      │
┌──────┴──────────────────────────┴──────────────────────┴─────────┐
│  Infrastructure: Config │ Cache │ Queue │ Database │ Observability│
└───────────────────────────────────────────────────────────────────┘
```

### Quick Start

```bash
# Install the CLI scaffold
cargo install rszeroctl

# Generate a new project
rszeroctl new my-service
cd my-service

# Run
cargo run
```

```rust
use rszero::prelude::*;
use axum::routing::get;
use serde::Serialize;

#[derive(Serialize)]
struct HelloResp {
    message: String,
}

async fn hello() -> impl axum::response::IntoResponse {
    JsonResponse::ok(HelloResp {
        message: "Hello from rszero!".into(),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = RszeroServer::new("0.0.0.0", 8080)
        .route("/hello", get(hello));
    server.start().await?;
    Ok(())
}
```

### Core Modules

| Module | Purpose | Stack |
|--------|---------|-------|
| `rest` | HTTP gateway & routing | Axum 0.7 |
| `rpc` | RPC client/server | Volo 0.12 (gRPC/Thrift) |
| `store` | Database ORM & migrations | sqlx + sea-orm |
| `cache` | Redis + in-memory caching | fred 6.0 |
| `queue` | Message queue | lapin (RabbitMQ) |
| `discovery` | Service registry | etcd-client |
| `trace` | Distributed tracing | OpenTelemetry |
| `metrics` | Prometheus metrics | prometheus 0.14 |
| `middleware` | Auth, logging, validation, caching | axum + jsonwebtoken |
| `breaker` | Circuit breaker | built-in |
| `limit` | Rate limiting | tower-governor |
| `retry` | Exponential backoff retry | built-in |
| `health` | Health & readiness probes | built-in |

### Documentation

- [Getting Started](docs/getting-started.md) — 5-minute tutorial
- [Architecture](docs/architecture.md) — System design & data flow
- [Best Practices](docs/best-practices.md) — Production guidelines
- [API Reference](docs/api-reference.md) — Module-level API docs
- [RPC Guide](docs/rpc-guide.md) — Volo gRPC integration

### Testing

```bash
# Run all tests
cargo test --workspace --all-features

# Run benchmarks
cargo bench

# Lint
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Security audit
cargo audit
```

### Roadmap

| Version | Target | ETA |
|---------|--------|-----|
| v0.1.0 | Core framework + rszeroctl basics | 2026 Q2 |
| v0.2.0 | Service governance + DB model codegen | 2026 Q3 |
| v0.5.0 | Production-stable + full test coverage | 2026 Q4 |
| v1.0.0 | Enterprise production validation | 2027 Q1 |

### License

MIT — see [LICENSE](LICENSE).

### Acknowledgements

- [Axum](https://github.com/tokio-rs/axum) — REST layer
- [Volo](https://github.com/cloudwego/volo) — RPC layer
- [SeaORM](https://github.com/SeaQL/sea-orm) — Database ORM
- [go-zero](https://github.com/zeromicro/go-zero) — Design inspiration for microservice patterns

---

<a id="中文"></a>
## 中文

### 概述

**rszero** 是一款面向 Rust 生态的云原生微服务框架，将现代 Rust 的内存安全、零成本抽象和高性能异步运行时，与经过生产验证的分布式系统模式相结合。

它提供了一套内聚、开箱即用的工具集，用于构建 REST API、RPC 服务以及完整的微服务架构——无需手动拼接数十个互不相关的 crate。

### 为什么选择 rszero？

| 能力 | 你得到的 |
|-----------|-------------|
| **内存安全** | `#![forbid(unsafe_code)]` — 整个框架 100% 安全 Rust |
| **类型安全** | 请求模式、API 契约和数据库查询的编译期保证 |
| **高性能** | 零 GC、低内存占用、基于 Tokio 的异步运行时与工作窃取调度器 |
| **模块化** | 特性门控编译 — 只为你使用的功能付费 (`rest`、`rpc`、`cache`、`store` 等) |
| **可观测性** | 内置结构化日志 (tracing)、Prometheus 指标、OpenTelemetry 链路追踪、健康探针 |
| **韧性** | 熔断器、限流、指数退避重试、负载脱落、超时控制 |
| **云原生** | etcd 服务发现、Docker 就绪、Kubernetes 健康端点、优雅关闭 |
| **开发体验** | `rszeroctl` 代码生成脚手架、统一 `prelude`、合理的默认值 |

### 架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        客户端 / 前端                              │
└───────────────────────────────┬─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│              rszero REST 网关（基于 Axum）                        │
│  路由 │ 鉴权 │ 校验 │ 限流 │ 熔断 │ 指标                          │
└───────────────────────────────┬─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│                    服务发现（etcd / 自定义）                       │
└─────────────┬─────────────────────┬──────────────────┬──────────┘
              │                     │                  │
┌─────────────▼──────┐  ┌──────────▼─────┐  ┌────────▼─────────┐
│  rszero RPC 用户   │  │ rszero RPC     │  │ rszero RPC 支付  │
│  服务（Volo）      │  │ 订单服务       │  │ 服务（Volo）     │
└─────────────────────┘  └─────────────────┘  └──────────────────┘
       ▲                          ▲                      ▲
       │                          │                      │
┌──────┴──────────────────────────┴──────────────────────┴─────────┐
│  基础设施层：配置 │ 缓存 │ 队列 │ 数据库 │ 可观测性                   │
└───────────────────────────────────────────────────────────────────┘
```

### 快速开始

```bash
# 安装 CLI 脚手架
cargo install rszeroctl

# 创建新项目
rszeroctl new my-service
cd my-service

# 运行
cargo run
```

```rust
use rszero::prelude::*;
use axum::routing::get;
use serde::Serialize;

#[derive(Serialize)]
struct HelloResp {
    message: String,
}

async fn hello() -> impl axum::response::IntoResponse {
    JsonResponse::ok(HelloResp {
        message: "Hello from rszero!".into(),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = RszeroServer::new("0.0.0.0", 8080)
        .route("/hello", get(hello));
    server.start().await?;
    Ok(())
}
```

### 核心模块

| 模块 | 用途 | 技术栈 |
|--------|---------|-------|
| `rest` | HTTP 网关与路由 | Axum 0.7 |
| `rpc` | RPC 客户端/服务端 | Volo 0.12 (gRPC/Thrift) |
| `store` | 数据库 ORM 与迁移 | sqlx + sea-orm |
| `cache` | Redis + 内存缓存 | fred 6.0 |
| `queue` | 消息队列 | lapin (RabbitMQ) |
| `discovery` | 服务注册发现 | etcd-client |
| `trace` | 分布式链路追踪 | OpenTelemetry |
| `metrics` | Prometheus 指标 | prometheus 0.14 |
| `middleware` | 鉴权、日志、校验、缓存 | axum + jsonwebtoken |
| `breaker` | 熔断降级 | 内置 |
| `limit` | 限流 | tower-governor |
| `retry` | 指数退避重试 | 内置 |
| `health` | 健康与就绪探针 | 内置 |

### 文档

- [入门教程](docs/getting-started.md) — 5 分钟上手
- [架构设计](docs/architecture.md) — 系统设计与数据流
- [最佳实践](docs/best-practices.md) — 生产级开发规范
- [API 参考](docs/api-reference.md) — 模块级 API 文档
- [RPC 指南](docs/rpc-guide.md) — Volo gRPC 集成详解

### 测试

```bash
# 运行全部测试
cargo test --workspace --all-features

# 运行基准测试
cargo bench

# 代码检查
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 安全审计
cargo audit
```

### 路线图

| 版本 | 目标 | 预计时间 |
|---------|--------|-----|
| v0.1.0 | 核心框架 + rszeroctl 基础 | 2026 Q2 |
| v0.2.0 | 服务治理 + 数据库模型代码生成 | 2026 Q3 |
| v0.5.0 | 生产级稳定 + 全量测试覆盖 | 2026 Q4 |
| v1.0.0 | 企业级生产验证 | 2027 Q1 |

### 许可证

MIT — 详见 [LICENSE](LICENSE)。

### 致谢

- [Axum](https://github.com/tokio-rs/axum) — REST 层
- [Volo](https://github.com/cloudwego/volo) — RPC 层
- [SeaORM](https://github.com/SeaQL/sea-orm) — 数据库 ORM
- [go-zero](https://github.com/zeromicro/go-zero) — 微服务模式的设计灵感来源
