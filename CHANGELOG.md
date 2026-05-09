# Changelog

All notable changes to rszero will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Graceful shutdown support for `RszeroServer` (SIGINT/SIGTERM)
- Log file output with daily rotation via `tracing-appender`
- Prometheus metrics module (`Metrics` with request counters, error counters, duration tracking)
- Health check module (`Health` with liveness/readiness tracking)
- `map_reduce_with_concurrency` for bounded parallel processing
- `FxStream::map` now actually transforms items
- `fastrand` for uniform random rejection in load shedder
- Feature flags for optional dependencies (`rest`, `rpc`, `cache`, `store`, `queue`, `discovery`, `trace`, `auth`)
- Integration test suite (`rszero/tests/integration_test.rs`)
- OpenAPI 3.0 specification generation (`OpenApiSpec`, `ApiOperation`, `SecurityScheme`)
- Request ID correlation middleware (`request_id_middleware`)
- Request validation middleware (`validation_middleware`)
- Body size limit middleware (`body_size_limit`)
- Distributed tracing middleware with span propagation (`trace_middleware`)
- Redis distributed lock (`DistributedLock`, `with_lock`)
- Cache-aside pattern helper (`cache_aside`)
- Configuration hot-reloading (`ConfigWatcher`)
- Database migration management (`Migrator`)
- Connection pool statistics (`PoolStats`)
- CORS configuration builder (`CorsConfig`)
- Response compression support (`RszeroServer::compression`)
- Retry mechanism with exponential backoff and jitter (`RetryPolicy`, `with_retry`)
- Volo 0.12 integration (volo-grpc, volo-thrift, volo-build)
- Comprehensive documentation (best-practices, architecture, getting-started, api-reference, rpc-guide, migration)

### Changed
- Decoupled `error` module from axum (HTTP response types gated behind `rest` feature)
- `RszeroError::status_code()` now returns `u16` instead of `axum::StatusCode`
- `FxStream` simplified from 2-type-param to single-type-param design
- Replaced `map_reduce_with_mapper` with `map_reduce_with_concurrency`
- Updated Volo dependency from 0.10 to 0.12
- Split `config/mod.rs` into `types.rs` + `mod.rs` for better maintainability
- Split `openapi/mod.rs` into `types.rs` + `mod.rs` for better maintainability
- MemCache now uses `dashmap` for lock-free concurrent access
- `JsonResponse::error()` now available on generic `JsonResponse<T>`
- Queue module now supports `connect()`, `ack()`, `nack()`, `pending_count()`

### Fixed
- Shedder rejection probability now uses `fastrand` instead of biased `Instant::now().elapsed()` hash
- RPC client builder now properly handles `ServiceDiscovery` with `Arc`
- Cache `del` method renamed to `delete` for consistency
- Bookstore example now uses separate caches for single items and lists
- Removed unused imports across multiple modules
- Fixed `FxStream` type parameter issues

### Documentation
- Added `README.md` — project overview, quick start, features
- Added `docs/best-practices.md` — 900-line comprehensive best practices guide
- Added `docs/architecture.md` — system architecture and data flow
- Added `docs/getting-started.md` — 5-minute tutorial for new users
- Added `docs/api-reference.md` — complete module API documentation
- Added `docs/rpc-guide.md` — Volo gRPC integration guide
- Added `docs/migration.md` — go-zero → rszero migration guide
- Added `SECURITY.md` — security policy and reporting
- Added `CODE_OF_CONDUCT.md` — community guidelines
- Added `CONTRIBUTING.md` — contribution guide
- Added `CHANGELOG.md` — version history
- Added `LICENSE` — MIT license

## [0.1.0] — 2026-04-03

### Added
- Initial release: rszero core framework with 14+ modules
- rszeroctl CLI scaffolding tool
- Bookstore microservice example (API Gateway + Book RPC + Order RPC)
- User-service example
- Unit tests (39), integration tests (7), doc tests (3)
- GitHub Actions CI workflow
- rustfmt.toml and clippy.toml configuration
- .gitignore for Rust projects
