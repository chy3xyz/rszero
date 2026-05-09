# Architecture / 架构设计

> System architecture, module design, and data flow.  
> 系统架构、模块设计和数据流。

---

## English

### 1. Overview

rszero adopts a layered architecture inspired by cloud-native microservice best practices: **Access Layer → Service Layer → Infrastructure Layer**. Each layer is independently scalable and feature-gated, so you compile only what you need.

```
┌──────────────────────────────────────────────────────────────┐
│                      Access Layer                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────┐ │
│  │   REST Gateway  │  │   gRPC Gateway  │  │  WebSocket   │ │
│  │   (Axum 0.7)    │  │   (Volo 0.12)   │  │   (planned)  │ │
│  └────────┬────────┘  └────────┬────────┘  └──────────────┘ │
└───────────┼────────────────────┼────────────────────────────┘
            │                    │
┌───────────▼────────────────────▼────────────────────────────┐
│                    Service Layer                              │
│  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────┐ │
│  │  User Service   │  │  Order Service  │  │  Pay Service │ │
│  │  (REST + RPC)   │  │  (REST + RPC)   │  │  (RPC only)  │ │
│  └────────┬────────┘  └────────┬────────┘  └──────────────┘ │
└───────────┼────────────────────┼────────────────────────────┘
            │                    │
┌───────────▼────────────────────▼────────────────────────────┐
│                  Infrastructure Layer                         │
│  ┌──────────┐ ┌──────┐ ┌───────┐ ┌──────┐ ┌──────────────┐  │
│  │ Config   │ │ Log  │ │ Cache │ │  DB  │ │   Queue      │  │
│  │(figment) │ │(trace│ │(fred) │ │(sqlx)│ │  (lapin)     │  │
│  └──────────┘ └──────┘ └───────┘ └──────┘ └──────────────┘  │
│  ┌──────────┐ ┌──────┐ ┌───────┐ ┌──────┐ ┌──────────────┐  │
│  │ Limit    │ │Break │ │Discov │ │Trace │ │   Metrics    │  │
│  │(governor)│ │(cb)  │ │(etcd) │ │(otel)│ │ (prometheus) │  │
│  └──────────┘ └──────┘ └───────┘ └──────┘ └──────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

### 2. Module Design

#### 2.1 Core Modules

| Module | Responsibility | Dependency | Status |
|--------|---------------|------------|--------|
| `rest` | HTTP server, routing, handlers | axum 0.7 | ✅ |
| `rpc` | RPC client/server | volo 0.12 | ✅ |
| `config` | Config loading, hot-reload | figment, dotenvy | ✅ |
| `log` | Structured logging | tracing | ✅ |
| `cache` | Redis + in-memory cache | fred 6.0, dashmap | ✅ |
| `queue` | Message queue | lapin | ✅ |
| `store` | Database connection, migrations | sqlx, sea-orm | ✅ |

#### 2.2 Governance Modules

| Module | Responsibility | Dependency | Status |
|--------|---------------|------------|--------|
| `limit` | Global rate limiting | tower-governor | ✅ |
| `breaker` | Circuit breaker | built-in | ✅ |
| `discovery` | Service registration & discovery | etcd-client | ✅ |
| `shedder` | Load shedding | built-in | ✅ |
| `timeout` | Timeout controls | tokio | ✅ |
| `retry` | Exponential backoff retry | built-in | ✅ |

#### 2.3 Cross-Cutting Modules

| Module | Responsibility | Dependency | Status |
|--------|---------------|------------|--------|
| `error` | Unified error handling | thiserror | ✅ |
| `middleware` | JWT/logging/tracing/validation | jsonwebtoken, axum | ✅ |
| `trace` | Distributed tracing | opentelemetry | ✅ |
| `health` | Health checks | built-in | ✅ |
| `metrics` | Prometheus metrics | built-in | ✅ |
| `openapi` | OpenAPI doc generation | serde, serde_yaml | ✅ |
| `concurrent` | MapReduce / functional streams | tokio | ✅ |

### 3. Data Flow

#### 3.1 Request Processing Pipeline

```
Client Request
    │
    ▼
┌─────────────────────┐
│  Load Shedder       │ ← Reject when overloaded
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  Request ID         │ ← Generate / propagate request ID
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  Trace Middleware   │ ← Distributed tracing
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  Validation         │ ← Request body validation
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  Rate Limiter       │ ← Rate limiting
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  JWT Auth           │ ← Authentication (optional)
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  Handler            │ ← Business logic
│  ├─ Cache Check     │
│  ├─ Circuit Breaker │
│  ├─ DB Query        │
│  └─ Cache Write     │
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  JsonResponse       │ ← Unified response format
└─────────────────────┘
```

#### 3.2 Cache-Aside Strategy

```
Request
    │
    ▼
┌──────────────┐     Hit    ┌──────────┐
│  Cache Get   │───────────▶│  Return  │
└──────┬───────┘            └──────────┘
       │ Miss
       ▼
┌──────────────┐     OK      ┌──────────┐
│  DB Query    │────────────▶│  Cache   │
└──────┬───────┘             │  Set+TTL │
       │ Error               └──────────┘
       ▼
┌──────────────┐
│  Error Resp  │
└──────────────┘
```

### 4. Configuration System

```
.rszero/
├── .env                    # Environment variables (dev)
├── .env.production         # Environment variables (prod)
├── etc/
│   ├── api.yaml            # API gateway config
│   └── user-rpc.yaml       # RPC service config
└── migrations/
    ├── 001_create_users.sql
    └── 002_create_orders.sql
```

Config loading priority (highest to lowest):
1. Environment variables (`RSZERO_*` prefix)
2. YAML config files
3. `.env` files
4. Default values

### 5. Error Handling Hierarchy

```
RszeroError (unified error type)
├── Config          ← Configuration errors
├── Database        ← Database errors
├── Cache           ← Cache errors
├── Rpc             ← RPC call errors
├── Http            ← HTTP errors (with status code)
├── Auth            ← Authentication errors
├── RateLimit       ← Rate limiting errors
├── CircuitBreaker  ← Circuit breaker errors
├── NotFound        ← Resource not found
├── Discovery       ← Service discovery errors
├── Queue           ← Message queue errors
├── Serialization   ← Serialization errors
└── Internal        ← Internal errors
```

### 6. Deployment Patterns

#### 6.1 Single Service

```
┌─────────────────────┐
│   Docker Container  │
│  ┌───────────────┐  │
│  │  rszero-app   │  │
│  │  :8080        │  │
│  └───────┬───────┘  │
└──────────┼──────────┘
           │
    ┌──────▼──────┐
    │   etcd      │
    │   Redis     │
    │   PostgreSQL│
    └─────────────┘
```

#### 6.2 Microservices with K8s

```
                    ┌─────────────┐
                    │   K8s LB    │
                    └──────┬──────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │ API GW 1 │ │ API GW 2 │ │ API GW 3 │
        └────┬─────┘ └────┬─────┘ └────┬─────┘
             │            │            │
        ┌────┴────────────┴────────────┴────┐
        │            etcd cluster            │
        └────┬────────────┬────────────┬────┘
             │            │            │
        ┌────▼─────┐ ┌────▼─────┐ ┌────▼─────┐
        │ User RPC │ │ Order RPC│ │ Pay RPC  │
        └──────────┘ └──────────┘ └──────────┘
```

---

## 中文

### 1. 整体架构

rszero 采用分层架构设计，灵感来源于云原生微服务最佳实践：**接入层 → 服务层 → 基础设施层**。每一层都可以独立扩展，并且通过特性门控按需编译，你只为你使用的功能买单。

```
┌──────────────────────────────────────────────────────────────┐
│                      接入层 (Access Layer)                    │
│  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────┐ │
│  │   REST 网关     │  │   gRPC 网关     │  │  WebSocket   │ │
│  │   (Axum 0.7)    │  │   (Volo 0.12)   │  │   (计划中)   │ │
│  └────────┬────────┘  └────────┬────────┘  └──────────────┘ │
└───────────┼────────────────────┼────────────────────────────┘
            │                    │
┌───────────▼────────────────────▼────────────────────────────┐
│                    服务层 (Service Layer)                     │
│  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────┐ │
│  │  用户服务       │  │  订单服务       │  │  支付服务    │ │
│  │  (REST + RPC)   │  │  (REST + RPC)   │  │  (仅 RPC)    │ │
│  └────────┬────────┘  └────────┬────────┘  └──────────────┘ │
└───────────┼────────────────────┼────────────────────────────┘
            │                    │
┌───────────▼────────────────────▼────────────────────────────┐
│                  基础设施层 (Infrastructure)                   │
│  ┌──────────┐ ┌──────┐ ┌───────┐ ┌──────┐ ┌──────────────┐  │
│  │ 配置中心 │ │ 日志 │ │ 缓存  │ │ 数据库│ │   消息队列   │  │
│  │(figment) │ │(trace│ │(fred) │ │(sqlx)│ │  (lapin)     │  │
│  └──────────┘ └──────┘ └───────┘ └──────┘ └──────────────┘  │
│  ┌──────────┐ ┌──────┐ ┌───────┐ ┌──────┐ ┌──────────────┐  │
│  │ 限流     │ │熔断  │ │服务发现│ │链路追踪│ │   指标     │  │
│  │(governor)│ │(cb)  │ │(etcd) │ │(otel)│ │ (prometheus) │  │
│  └──────────┘ └──────┘ └───────┘ └──────┘ └──────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

### 2. 模块设计

#### 2.1 核心模块

| 模块 | 职责 | 依赖 | 状态 |
|------|------|------|------|
| `rest` | HTTP 服务器、路由、Handler | axum 0.7 | ✅ |
| `rpc` | RPC 客户端/服务端 | volo 0.12 | ✅ |
| `config` | 配置加载、热重载 | figment, dotenvy | ✅ |
| `log` | 结构化日志 | tracing | ✅ |
| `cache` | Redis + 内存缓存 | fred 6.0, dashmap | ✅ |
| `queue` | 消息队列 | lapin | ✅ |
| `store` | 数据库连接、迁移 | sqlx, sea-orm | ✅ |

#### 2.2 治理模块

| 模块 | 职责 | 依赖 | 状态 |
|------|------|------|------|
| `limit` | 全局限流 | tower-governor | ✅ |
| `breaker` | 熔断降级 | 内置 | ✅ |
| `discovery` | 服务注册发现 | etcd-client | ✅ |
| `shedder` | 负载脱落 | 内置 | ✅ |
| `timeout` | 超时控制 | tokio | ✅ |
| `retry` | 指数退避重试 | 内置 | ✅ |

#### 2.3 横切模块

| 模块 | 职责 | 依赖 | 状态 |
|------|------|------|------|
| `error` | 统一错误处理 | thiserror | ✅ |
| `middleware` | JWT/日志/追踪/校验 | jsonwebtoken, axum | ✅ |
| `trace` | 分布式链路追踪 | opentelemetry | ✅ |
| `health` | 健康检查 | 内置 | ✅ |
| `metrics` | Prometheus 指标 | 内置 | ✅ |
| `openapi` | OpenAPI 文档生成 | serde, serde_yaml | ✅ |
| `concurrent` | MapReduce / 函数式流 | tokio | ✅ |

### 3. 数据流

#### 3.1 请求处理流程

```
客户端请求
    │
    ▼
┌─────────────────────┐
│  负载脱落           │ ← 系统过载时拒绝请求
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  请求 ID            │ ← 生成/传播请求 ID
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  链路追踪中间件     │ ← 分布式追踪
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  请求校验           │ ← 请求体校验
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  限流器             │ ← 限流
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  JWT 鉴权           │ ← 认证（可选）
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  Handler            │ ← 业务逻辑
│  ├─ 缓存查询        │
│  ├─ 熔断器          │
│  ├─ 数据库查询      │
│  └─ 缓存写入        │
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  JsonResponse       │ ← 统一响应格式
└─────────────────────┘
```

#### 3.2 缓存策略

```
请求
    │
    ▼
┌──────────────┐     命中   ┌──────────┐
│  缓存查询    │──────────▶│  返回    │
└──────┬───────┘           └──────────┘
       │ 未命中
       ▼
┌──────────────┐     成功   ┌──────────┐
│  数据库查询  │──────────▶│  缓存    │
└──────┬───────┘           │  写入TTL │
       │ 失败              └──────────┘
       ▼
┌──────────────┐
│  错误响应    │
└──────────────┘
```

### 4. 配置体系

```
.rszero/
├── .env                    # 环境变量（开发）
├── .env.production         # 环境变量（生产）
├── etc/
│   ├── api.yaml            # API 网关配置
│   └── user-rpc.yaml       # RPC 服务配置
└── migrations/
    ├── 001_create_users.sql
    └── 002_create_orders.sql
```

配置加载优先级（从高到低）：
1. 环境变量 (`RSZERO_*` 前缀)
2. YAML 配置文件
3. `.env` 文件
4. 默认值

### 5. 错误处理体系

```
RszeroError (统一错误类型)
├── Config          ← 配置错误
├── Database        ← 数据库错误
├── Cache           ← 缓存错误
├── Rpc             ← RPC 调用错误
├── Http            ← HTTP 错误（带状态码）
├── Auth            ← 认证错误
├── RateLimit       ← 限流错误
├── CircuitBreaker  ← 熔断错误
├── NotFound        ← 资源不存在
├── Discovery       ← 服务发现错误
├── Queue           ← 消息队列错误
├── Serialization   ← 序列化错误
└── Internal        ← 内部错误
```

### 6. 部署架构

#### 6.1 单服务部署

```
┌─────────────────────┐
│   Docker 容器       │
│  ┌───────────────┐  │
│  │  rszero-app   │  │
│  │  :8080        │  │
│  └───────┬───────┘  │
└──────────┼──────────┘
           │
    ┌──────▼──────┐
    │   etcd      │
    │   Redis     │
    │   PostgreSQL│
    └─────────────┘
```

#### 6.2 微服务 K8s 部署

```
                    ┌─────────────┐
                    │   K8s LB    │
                    └──────┬──────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │ API GW 1 │ │ API GW 2 │ │ API GW 3 │
        └────┬─────┘ └────┬─────┘ └────┬─────┘
             │            │            │
        ┌────┴────────────┴────────────┴────┐
        │            etcd cluster            │
        └────┬────────────┬────────────┬────┘
             │            │            │
        ┌────▼─────┐ ┌────▼─────┐ ┌────▼─────┐
        │ User RPC │ │ Order RPC│ │ Pay RPC  │
        └──────────┘ └──────────┘ └──────────┘
```
