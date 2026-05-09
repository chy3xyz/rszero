//! RPC service layer — Volo gRPC/Thrift wrapper replicating go-zero zrpc.
//!
//! Provides RPC client and server abstractions with service discovery,
//! load balancing, timeout, and circuit breaker integration.
//!
//! # Volo Integration
//!
//! rszero uses CloudWeGo Volo (volo-grpc, volo-thrift) as the RPC transport layer.
//! Users define their service in `.proto` or `.thrift` IDL files, then use
//! `volo-build` to generate Rust code. rszero provides the configuration,
//! service discovery, and lifecycle management around the generated code.
//!
//! # Example
//!
//! ```no_run
//! use rszero::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = load_config("etc/rpc.yaml")?;
//!     let server = RpcServer::from_config(&config);
//!     server.start().await?;
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod server;
pub mod interceptor;

pub use client::{RpcClient, RpcClientBuilder};
pub use server::RpcServer;
pub use interceptor::{Interceptor, InterceptorChain, RpcContext, LoggingInterceptor, MetricsInterceptor, TimeoutInterceptor, RetryInterceptor};
