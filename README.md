# rszero

> Rust + Service + Zero — 1:1 Rust 复刻 go-zero 一站式微服务框架

[![CI](https://github.com/your-org/rszero/actions/workflows/ci.yml/badge.svg)](https://github.com/your-org/rszero/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/rszero.svg)](https://crates.io/crates/rszero)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-blue.svg)](https://www.rust-lang.org)

---

## 概述

**rszero** 是基于 Axum (REST) + Volo (RPC) 构建的企业级微服务框架，完全对齐 go-zero 的设计理念与开发体验。

### 核心特性

- **零学习成本** — go-zero 用户零迁移成本，API 设计 1:1 对齐
- **代码生成优先** — 配套 `rszeroctl` 脚手架（对标 goctl），一键生成项目骨架
- **开箱即用** — 全链路生产级能力无需手动拼接 Rust 生态组件
- **极致性能** — 无 GC、低内存占用、高并发低延迟
- **类型安全** — Rust 编译期检查，零 unsafe 代码
- **云原生** — 内置服务发现、负载均衡、熔断降级、链路追踪

### 架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        客户端/前端/上游服务                      │
└───────────────────────────────┬─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│              rszero-rest  API 网关（Axum 内核）                 │
│  路由分发、鉴权、参数校验、限流、熔断、日志、链路追踪、协议转换  │
└───────────────────────────────┬─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│                    服务注册与发现中心（etcd/nacos）              │
└─────────────┬─────────────────────┬──────────────────┬──────────┘
              │                     │                  │
┌─────────────▼──────┐  ┌──────────▼─────┐  ┌────────▼─────────┐
│ rszero-rpc 用户服务 │  │ rszero-rpc订单服务│  │ rszero-rpc支付服务│
│    （Volo 内核）    │  │   （Volo 内核）  │  │   （Volo 内核）  │
└─────────────────────┘  └─────────────────┘  └───────────────────┘
       ▲                          ▲                      ▲
       │                          │                      │
┌──────┴──────────────────────────┴──────────────────────┴─────────┐
│  基础设施层：配置中心、缓存、消息队列、数据库、可观测性、分布式锁  │
└─────────────────────────────────────────────────────────────────────┘
```

### 组件对照

| go-zero 组件 | rszero 实现 | 状态 |
|-------------|------------|------|
| rest | `rszero::rest` (Axum 0.7) | ✅ |
| zrpc | `rszero::rpc` (Volo 0.12) | ✅ |
| queue | `rszero::queue` (lapin) | ✅ |
| store | `rszero::store` (sqlx + sea-orm) | ✅ |
| trace | `rszero::trace` (OpenTelemetry) | ✅ |
| goctl | `rszeroctl` | ✅ |

> ✅ 生产可用 | 🚧 开发中

---

## 快速开始

### 环境要求

- Rust 1.75+
- PostgreSQL / MySQL (可选)
- Redis (可选)
- etcd (可选)

### 安装

```bash
# 克隆仓库
git clone https://github.com/your-org/rszero.git
cd rszero

# 编译
cargo build --release

# 安装脚手架
cargo install --path rszeroctl
```

### 创建项目

```bash
# 生成项目脚手架
rszeroctl new my-project
cd my-project

# 运行
cargo run
```

### 最小示例

```rust
use rszero::prelude::*;
use serde::{Deserialize, Serialize};
use axum::routing::get;

#[derive(Serialize)]
struct HelloResp {
    message: String,
}

async fn hello() -> impl axum::response::IntoResponse {
    JsonResponse::ok(HelloResp {
        message: "Hello, rszero!".into(),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = rszero::config::RszeroConfig::default();
    log::init(&config.log);

    let server = RszeroServer::new("0.0.0.0", 8080)
        .route("/hello", get(hello));

    server.start().await?;
    Ok(())
}
```

---

## 项目结构

```
rszero/
├── Cargo.toml                    # Workspace 配置
├── README.md                     # 本文档
├── CHANGELOG.md                  # 版本历史
├── CONTRIBUTING.md               # 贡献指南
├── LICENSE                       # MIT 许可证
├── docs/
│   ├── best-practices.md         # 最佳实践
│   ├── architecture.md           # 架构设计
│   └── getting-started.md        # 入门教程
├── rszero/                       # 核心框架 crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                # 统一导出 (prelude)
│       ├── rest/                 # REST API 网关
│       ├── rpc/                  # RPC 服务
│       ├── config/               # 配置管理
│       ├── log/                  # 结构化日志
│       ├── cache/                # 缓存层
│       ├── queue/                # 消息队列
│       ├── store/                # 数据库/ORM
│       ├── limit/                # 限流
│       ├── breaker/              # 熔断
│       ├── discovery/            # 服务发现
│       ├── middleware/           # 中间件
│       ├── trace/                # 链路追踪
│       ├── retry/                # 重试机制
│       ├── shedder/              # 负载脱落
│       ├── timeout/              # 超时控制
│       ├── health/               # 健康检查
│       ├── metrics/              # Prometheus 指标
│       ├── openapi/              # OpenAPI 文档生成
│       ├── concurrent/           # 并发工具
│       ├── error/                # 错误处理
│       └── utils/                # 工具函数
├── rszeroctl/                    # 代码生成脚手架
└── examples/
    ├── user-service/             # 简单示例
    └── bookstore/                # 完整微服务示例
```

---

## 文档

- [最佳实践](docs/best-practices.md) — 对齐 go-zero 的生产级实践（900 行）
- [架构设计](docs/architecture.md) — 系统设计详解
- [入门教程](docs/getting-started.md) — 5 分钟从零开始构建微服务
- [API 参考](docs/api-reference.md) — 完整模块 API 文档
- [RPC 指南](docs/rpc-guide.md) — Volo gRPC 集成详解
- [迁移指南](docs/migration.md) — go-zero → rszero 零成本迁移

---

## 测试

```bash
# 运行所有测试
cargo test --workspace

# 运行基准测试
cargo bench

# 运行 clippy
cargo clippy --workspace --all-targets -- -D warnings
```

**测试覆盖**：81 个单元测试 + 14 个集成测试 + 3 个文档测试 + 14 个示例测试 = 112 个测试全通过

---

## 路线图

| 版本 | 目标 | ETA |
|------|------|-----|
| v0.1.0 | 核心框架 + rszeroctl 基础 | 2026Q2 |
| v0.2.0 | 服务治理 + DB 模型生成 | 2026Q3 |
| v0.5.0 | 生产级稳定 + 全量测试 | 2026Q4 |
| v1.0.0 | 企业级生产验证 | 2027Q1 |

---

## 许可证

本项目采用 MIT 许可证 — 详见 [LICENSE](LICENSE) 文件。

---

## 贡献

详见 [CONTRIBUTING.md](CONTRIBUTING.md)。

---

## 致谢

- [go-zero](https://github.com/zeromicro/go-zero) — 设计灵感来源
- [Axum](https://github.com/tokio-rs/axum) — REST 内核
- [Volo](https://github.com/cloudwego/volo) — RPC 内核
