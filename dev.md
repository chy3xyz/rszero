# rszero 完整方案：Rust 生态 1:1 复刻 go-zero 一站式微服务框架
## 一、项目核心定位
**rszero** = **R**ust + **S**ervice + **Zero**，完全对齐 go-zero 设计理念与开发体验，是基于 Axum + Volo 构建的**企业级一站式微服务框架**，主打：
- 「零额外配置、约定优于配置」，go-zero 用户零学习成本无缝迁移
- 代码生成优先，配套 `rszeroctl` 脚手架（对标 goctl）
- 全链路生产级能力开箱即用，无需手动拼接 Rust 生态组件
- 极致性能：无 GC、低内存占用、高并发低延迟，全面超越 go-zero 性能表现

---

## 二、整体架构（1:1 复刻 go-zero 经典架构）
完全沿用 go-zero 业界验证的「API 网关层 + RPC 微服务层」分层架构，配套完整的服务治理与基础设施，架构图与 go-zero 完全一致：
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

---

## 三、项目完整目录结构（符合 Rust 工作空间规范 + go-zero 目录约定）
### 3.1 框架核心仓库结构（rszero 本体）
```
rszero/
├── Cargo.toml                          # 框架工作空间根配置
├── README.md                           # 文档（对齐 go-zero 官方文档）
├── rszero/                             # 框架核心主 crate（用户一键引入）
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                      # 统一导出所有模块
│       ├── rest/                       # 对标 go-zero rest（Axum 封装）
│       ├── rpc/                        # 对标 go-zero zrpc（Volo 封装）
│       ├── config/                     # 对标 go-zero config（多环境配置）
│       ├── log/                        # 对标 go-zero log（结构化日志）
│       ├── cache/                      # 对标 go-zero cache（分布式缓存）
│       ├── queue/                      # 对标 go-zero queue（消息队列）
│       ├── store/                      # 对标 go-zero store（数据库/ORM）
│       ├── limit/                      # 对标 go-zero limit（全局限流）
│       ├── breaker/                    # 对标 go-zero breaker（熔断降级）
│       ├── discovery/                  # 对标 go-zero discovery（服务发现）
│       ├── middleware/                 # 通用中间件（鉴权、日志、链路追踪）
│       ├── trace/                      # 链路追踪（OpenTelemetry）
│       └── error/                      # 全局统一错误处理
├── rszeroctl/                          # 对标 goctl，代码生成脚手架
├── examples/                           # 官方示例（用户服务、订单服务、网关）
└── tests/                              # 全量单元测试、集成测试
```

### 3.2 用户业务项目标准结构（go-zero 用户完全熟悉）
用户通过 `rszeroctl` 一键生成的业务项目，目录和 go-zero 1:1 对齐：
```
your-project/
├── .env                                # 环境配置
├── Cargo.toml                          # Rust 工作空间配置
├── etc/                                # 配置文件目录（对标 go-zero etc）
│   ├── api.yaml                        # 网关配置
│   └── user-rpc.yaml                   # RPC 服务配置
├── api/                                # API 网关层（对标 go-zero api）
│   ├── desc/                           # API 定义文件（对标 go-zero api desc）
│   ├── handler/                        # HTTP 处理器
│   ├── middleware/                     # 网关中间件
│   ├── types/                          # 请求/响应类型定义
│   └── main.rs                         # 网关启动入口
├── rpc/                                # RPC 微服务层（对标 go-zero rpc）
│   ├── user/                           # 用户微服务
│   │   ├── desc/                       # Thrift/Protobuf IDL 文件
│   │   ├── logic/                      # 业务逻辑层
│   │   ├── svc/                        # 服务上下文
│   │   ├── model/                      # 数据库模型
│   │   └── main.rs                     # RPC 服务启动入口
│   └── order/                          # 订单微服务（同上）
├── idl/                                # 公共 IDL 文件
├── common/                             # 公共依赖包（错误码、工具函数）
└── deploy/                             # 部署配置（Docker、K8s）
```

---

## 四、核心 Crate 与依赖选型（精准对齐 go-zero 组件）
### 4.1 根 Cargo.toml 工作空间配置
```toml
[workspace]
members = [
    "rszero",
    "rszeroctl",
    "examples/*",
]
resolver = "2"

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT"
repository = "https://github.com/your-org/rszero"

[workspace.dependencies]
# 核心框架内核
axum = { version = "0.7", features = ["full"] }
volo = { version = "0.10", features = ["server", "client", "thrift", "protobuf"] }
tokio = { version = "1.0", features = ["full"] }

# 配置管理
figment = { version = "0.10", features = ["env", "toml", "yaml"] }
dotenvy = "0.15"

# 数据库 & ORM
sqlx = { version = "0.7", features = ["postgres", "mysql", "runtime-tokio", "serde_json"] }
sea-orm = { version = "0.12", features = ["sqlx-postgres", "sqlx-mysql", "runtime-tokio"] }

# 缓存 & 分布式锁
fred = { version = "6.0", features = ["tokio-runtime", "serde-json", "pool"] }
redlock = "0.3"

# 消息队列
lapin = { version = "2.0", features = ["tokio", "rustls"] } # RabbitMQ
redis-queue = "0.4" # Redis 轻量队列

# 服务治理
tower = { version = "0.4", features = ["full"] }
tower-http = { version = "0.5", features = ["full"] }
tower-governor = "0.3" # 限流
volo-breaker = "0.2" # 熔断
volo-timeout = "0.2" # 超时
volo-loadbalance = "0.3" # 负载均衡

# 服务发现
volo-discovery = "0.3"
volo-discovery-etcd = "0.3"
volo-discovery-nacos = "0.3"

# 日志 & 链路追踪
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json", "fmt"] }
tracing-appender = "0.2"
opentelemetry = { version = "0.20", features = ["trace", "metrics"] }

# 鉴权 & 安全
jsonwebtoken = "9.0"
bcrypt = "0.15"

# 通用工具
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
once_cell = "1.19"
async-trait = "0.1"
```

### 4.2 核心模块 1:1 对齐 go-zero 实现
#### rszero/src/lib.rs（统一导出，用户一键引入）
```rust
//! rszero: Rust 一站式微服务框架，对标 go-zero
#![forbid(unsafe_code)]
#![warn(missing_docs)]

// 核心服务层
pub mod rest;
pub mod rpc;

// 基础设施层
pub mod config;
pub mod log;
pub mod cache;
pub mod queue;
pub mod store;
pub mod discovery;

// 服务治理层
pub mod limit;
pub mod breaker;
pub mod middleware;
pub mod trace;

// 通用工具
pub mod error;
pub mod utils;

// 全局预导入（用户只需 use rszero::prelude::*; 即可使用所有核心能力）
pub mod prelude {
    pub use super::config::*;
    pub use super::error::*;
    pub use super::log::*;
    pub use super::rest::*;
    pub use super::rpc::*;
    pub use super::cache::*;
    pub use super::queue::*;
    pub use super::store::*;
    pub use super::middleware::*;
}
```

---

## 五、核心工具链：rszeroctl（对标 goctl，框架核心竞争力）
完全复刻 goctl 的核心能力，是 rszero 的开发效率核心，支持：
### 5.1 核心功能
| 功能 | 对应 goctl 能力 | 说明 |
|------|----------------|------|
| 项目脚手架一键生成 | `goctl template init` | 一键生成完整的网关+RPC微服务项目结构 |
| API 定义代码生成 | `goctl api go` | 解析 .api 定义文件，一键生成 Axum 网关路由、handler、类型定义 |
| RPC IDL 代码生成 | `goctl rpc protoc` | 解析 Thrift/Protobuf IDL，一键生成 Volo RPC 服务端、客户端代码、业务分层 |
| 数据库模型生成 | `goctl model mysql` | 连接数据库，一键生成 Model 层代码、CRUD 封装 |
| 部署文件生成 | `goctl docker` | 一键生成 Dockerfile、K8s 部署清单 |
| 文档生成 | `goctl doc` | 自动生成 OpenAPI 接口文档 |

### 5.2 安装与使用命令（和 goctl 完全一致）
```bash
# 安装 rszeroctl
cargo install rszeroctl

# 1. 一键生成 API 网关项目（对标 goctl api go）
rszeroctl api go --api desc/user.api --dir ./api

# 2. 一键生成 RPC 微服务（对标 goctl rpc protoc）
rszeroctl rpc protoc ./idl/user.proto --go_out=./rpc/user --go-grpc_out=./rpc/user

# 3. 一键生成数据库 Model 代码（对标 goctl model mysql）
rszeroctl model mysql datasource --url "mysql://root:123456@localhost:3306/test" --table "users" --dir ./model

# 4. 一键生成 Dockerfile
rszeroctl docker --go main.rs --out Dockerfile
```

---

## 六、5分钟快速上手（和 go-zero 开发流程完全一致）
### 步骤1：环境准备
```bash
# 安装 Rust 工具链
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装 rszeroctl
cargo install rszeroctl

# 启动依赖服务（etcd、Redis、MySQL）
docker run -d --name etcd -p 2379:2379 bitnami/etcd:latest
docker run -d --name redis -p 6379:6379 redis
docker run -d --name mysql -p 3306:3306 -e MYSQL_ROOT_PASSWORD=123456 mysql:8.0
```

### 步骤2：一键生成项目
```bash
# 生成项目脚手架
rszeroctl new rszero-demo
cd rszero-demo
```

### 步骤3：编写 API 定义（对标 go-zero .api 文件）
新建 `api/desc/user.api`：
```
type (
    GetUserReq {
        Uid int64 `path:"uid"`
    }
    GetUserResp {
        Uid  int64  `json:"uid"`
        Name string `json:"name"`
        Age  int32  `json:"age"`
    }
)

@server(
    prefix: /v1
    group: user
)
service user-api {
    @handler getUser
    get /user/:uid (GetUserReq) returns (GetUserResp)
}
```

### 步骤4：一键生成网关代码
```bash
rszeroctl api go --api api/desc/user.api --dir ./api
```
自动生成：路由、handler、请求/响应类型、中间件、启动入口，无需手动写一行模板代码。

### 步骤5：编写 RPC IDL 与生成服务
新建 `idl/user.thrift`：
```thrift
namespace rs user.service

struct GetUserReq {
    1: required i64 uid
}
struct GetUserResp {
    1: required i64 uid
    2: required string name
    3: required i32 age
}
service UserService {
    GetUserResp GetUser(1: GetUserReq req)
}
```

生成 RPC 服务：
```bash
rszeroctl rpc protoc idl/user.thrift --out ./rpc/user
```
自动生成：Volo RPC 服务端、客户端、业务逻辑分层、服务注册发现配置。

### 步骤6：编写业务逻辑
在 `rpc/user/logic/get_user_logic.rs` 中编写业务逻辑，自动集成缓存、数据库、队列能力：
```rust
use rszero::prelude::*;
use user_service::*;

pub struct GetUserLogic;

impl GetUserLogic {
    pub async fn execute(&self, req: GetUserReq) -> Result<GetUserResp, RszeroError> {
        let uid = req.uid;
        let cache_key = format!("user:{}", uid);

        // 1. 查缓存（开箱即用，无需手动初始化）
        if let Some(cache_data) = cache().get::<String>(&cache_key).await? {
            return Ok(serde_json::from_str(&cache_data)?);
        }

        // 2. 查数据库（自动生成的 Model 层）
        let user = model::User::find_by_uid(db(), uid).await?;

        // 3. 构建响应
        let resp = GetUserResp {
            uid: user.uid,
            name: user.name,
            age: user.age,
        };

        // 4. 回写缓存
        cache().set_ex(&cache_key, serde_json::to_string(&resp)?, 3600).await?;

        // 5. 投递消息队列
        queue().push("user_login", &resp).await?;

        Ok(resp)
    }
}
```

### 步骤7：启动服务
```bash
# 启动 RPC 服务
cd rpc/user && cargo run

# 启动 API 网关
cd api && cargo run

# 测试接口
curl http://localhost:8080/v1/user/1001
```

---

## 七、全组件能力与 go-zero 精准对照表
| go-zero 核心组件 | rszero 对应实现 | 能力完全对齐 |
|------------------|----------------|--------------|
| go-zero rest | rszero::rest（Axum 封装） | ✅ 路由、handler、中间件、参数绑定、自动文档 |
| go-zero zrpc | rszero::rpc（Volo 封装） | ✅ RPC 服务/客户端、服务发现、负载均衡、超时重试 |
| go-zero config | rszero::config | ✅ 多环境配置、热加载、多格式支持（yaml/toml/env） |
| go-zero log | rszero::log | ✅ 结构化日志、分级日志、文件切割、链路追踪集成 |
| go-zero cache | rszero::cache | ✅ Redis 缓存、分布式锁、缓存穿透/击穿/雪崩防护 |
| go-zero queue | rszero::queue | ✅ Redis 队列、RabbitMQ、延迟队列、死信队列 |
| go-zero store/model | rszero::store | ✅ MySQL/PostgreSQL 支持、ORM 封装、CRUD 代码生成、连接池 |
| go-zero limit | rszero::limit | ✅ 全局限流、IP 限流、接口限流、令牌桶/漏桶算法 |
| go-zero breaker | rszero::breaker | ✅ 熔断器、降级策略、自适应熔断 |
| go-zero discovery | rszero::discovery | ✅ etcd/nacos 服务注册发现、健康检查 |
| go-zero jwt | rszero::middleware::jwt | ✅ JWT 鉴权、自动刷新、白名单 |
| go-zero trace | rszero::trace | ✅ 全链路追踪、OpenTelemetry 兼容、Jaeger/Grafana 集成 |
| goctl | rszeroctl | ✅ 全场景代码生成、脚手架、部署文件生成 |

---

## 八、生产级特性与部署方案
### 8.1 生产级核心特性
1. **全链路可观测性**：内置 Prometheus metrics、Grafana 看板、链路追踪、日志告警
2. **高可用保障**：熔断、降级、限流、重试、超时、负载均衡、故障隔离
3. **安全防护**：防 SQL 注入、XSS 防护、CSRF 防护、签名验签、数据加密
4. **兼容性**：完全兼容 gRPC、Thrift 协议，可与 go-zero 服务无缝互通
5. **极致性能优化**：连接池复用、零拷贝、预编译、内存池优化，性能远超 go-zero

### 8.2 部署方案
1. **容器化部署**：`rszeroctl` 一键生成优化的 Dockerfile，基于 Alpine 镜像，打包后镜像大小 < 20MB
2. **K8s 部署**：自动生成 Deployment、Service、ConfigMap、HPA 配置，支持滚动更新、弹性扩缩容
3. **服务网格**：兼容 Istio、Linkerd 服务网格，支持流量治理、灰度发布
4. **配置中心**：支持 Nacos/Apollo 配置中心，配置热加载，无需重启服务

---

## 九、go-zero → rszero 无缝迁移指南
1. **API 定义无缝迁移**：go-zero 的 `.api` 文件可直接复用，`rszeroctl` 完全兼容 go-zero 的 API 语法
2. **IDL 文件无缝迁移**：Protobuf/Thrift IDL 直接复用，无需修改
3. **业务逻辑迁移**：go-zero 的分层逻辑（handler → logic → model）完全一致，只需将 Go 代码转为 Rust 代码
4. **配置文件无缝迁移**：go-zero 的 yaml 配置文件可直接复用，rszero 完全兼容 go-zero 的配置结构
5. **部署方式无缝迁移**：原有的 Docker/K8s 部署方案几乎无需修改，仅替换镜像即可

---

## 十、项目 Roadmap
| 版本 | 核心目标 | 完成时间 |
|------|----------|----------|
| v0.1.0 | 核心框架能力落地（rest/rpc/配置/日志/缓存）、rszeroctl 基础代码生成 | 2026Q2 |
| v0.2.0 | 全量服务治理能力（限流/熔断/链路追踪）、数据库模型生成 | 2026Q3 |
| v0.5.0 | 生产级稳定版、全量测试覆盖、官方文档完善、最佳实践 | 2026Q4 |
| v1.0.0 | 正式稳定版、企业级生产验证、全生态组件完善 | 2027Q1 |
