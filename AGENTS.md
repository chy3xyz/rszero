# rszero PROJECT KNOWLEDGE BASE

**Generated:** 2026-04-03
**Branch:** main
**State:** Greenfield — design spec only (`dev.md`), no source code yet.

## OVERVIEW
rszero = Rust + Service + Zero. 1:1 Rust复刻 go-zero 一站式微服务框架。基于 Axum (REST) + Volo (RPC) 构建的企业级微服务框架。目标：go-zero 用户零学习成本迁移。

## STRUCTURE
```
rszero/                          # 框架核心仓库 (Cargo workspace)
├── rszero/                      # 核心 crate — 用户一键引入
│   └── src/
│       ├── lib.rs               # 统一导出 (prelude)
│       ├── rest/                # Axum 封装 → 对标 go-zero rest
│       ├── rpc/                 # Volo 封装 → 对标 go-zero zrpc
│       ├── config/              # 多环境配置 (figment + dotenvy)
│       ├── log/                 # 结构化日志 (tracing)
│       ├── cache/               # Redis 缓存 (fred)
│       ├── queue/               # 消息队列 (lapin / redis-queue)
│       ├── store/               # 数据库/ORM (sqlx + sea-orm)
│       ├── limit/               # 全局限流 (tower-governor)
│       ├── breaker/             # 熔断降级 (volo-breaker)
│       ├── discovery/           # 服务发现 (etcd/nacos via volo-discovery)
│       ├── middleware/          # 通用中间件 (JWT, 日志, 链路追踪)
│       ├── trace/               # OpenTelemetry 链路追踪
│       └── error/               # 全局统一错误处理 (thiserror)
├── rszeroctl/                   # 代码生成脚手架 → 对标 goctl
├── examples/                    # 官方示例
└── tests/                       # 单元/集成测试
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| REST 网关 | `rszero/src/rest/` | Axum 封装 |
| RPC 服务 | `rszero/src/rpc/` | Volo + Thrift/Protobuf |
| 配置管理 | `rszero/src/config/` | figment (yaml/toml/env) |
| 缓存 | `rszero/src/cache/` | fred (Redis) |
| 数据库 | `rszero/src/store/` | sqlx + sea-orm |
| 代码生成 | `rszeroctl/` | 对标 goctl 全量能力 |
| 错误处理 | `rszero/src/error/` | thiserror, 全局 RszeroError |

## CONVENTIONS
- **模块命名**: 1:1 对齐 go-zero 组件名 (rest, rpc, config, log, cache, queue, store, limit, breaker, discovery)
- **导出风格**: `pub mod prelude` — 用户 `use rszero::prelude::*` 一键引入
- **安全**: `#![forbid(unsafe_code)]` — 零 unsafe
- **Cargo workspace**: resolver = "2", edition = "2021"
- **业务项目结构**: 用户通过 rszeroctl 生成的项目目录与 go-zero 1:1 对齐 (api/, rpc/, idl/, common/, deploy/, etc/)

## ANTI-PATTERNS (THIS PROJECT)
- **NEVER** use `unsafe` — `#![forbid(unsafe_code)]`
- **NEVER** deviate from go-zero directory conventions in generated projects
- **NEVER** require manual component wiring — all capabilities auto-integrated via prelude
- **DO NOT** create new top-level crates outside workspace structure

## DEPENDENCY SELECTION
| Layer | Crate | Version |
|-------|-------|---------|
| REST | axum | 0.7 |
| RPC | volo | 0.10 |
| Runtime | tokio | 1.0 |
| Config | figment | 0.10 |
| DB | sqlx + sea-orm | 0.7 + 0.12 |
| Cache | fred | 6.0 |
| Queue | lapin | 2.0 |
| Rate Limit | tower-governor | 0.3 |
| Circuit Breaker | volo-breaker | 0.2 |
| Discovery | volo-discovery-etcd/nacos | 0.3 |
| Tracing | tracing + opentelemetry | 0.1 + 0.20 |
| Auth | jsonwebtoken | 9.0 |
| Error | thiserror | 1.0 |

## COMMANDS
```bash
# 安装脚手架
cargo install rszeroctl

# 生成项目
rszeroctl new my-project

# API 代码生成
rszeroctl api go --api desc/user.api --dir ./api

# RPC 代码生成
rszeroctl rpc protoc idl/user.proto --out ./rpc/user

# Model 代码生成
rszeroctl model mysql datasource --url "..." --table "users" --dir ./model
```

## ROADMAP
| Version | Target | ETA |
|---------|--------|-----|
| v0.1.0 | 核心框架 + rszeroctl 基础 | 2026Q2 |
| v0.2.0 | 服务治理 + DB 模型生成 | 2026Q3 |
| v0.5.0 | 生产级稳定 + 全量测试 | 2026Q4 |
| v1.0.0 | 企业级生产验证 | 2027Q1 |

## NOTES
- go-zero `.api` 文件语法完全兼容，可直接复用
- Protobuf/Thrift IDL 无需修改即可使用
- 配置文件 (yaml) 与 go-zero 结构兼容
- 与 go-zero 服务通过 gRPC/Thrift 无缝互通
