# Contributing to rszero

Thank you for your interest in contributing to rszero!

## Development Setup

```bash
# Install Rust (minimal profile)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --profile minimal -y

# Clone and build
git clone https://github.com/your-org/rszero.git
cd rszero
cargo build

# Run tests
cargo test --workspace

# Run clippy
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all
```

## Code Style

- Follow `rustfmt` defaults (see `rustfmt.toml`)
- All public APIs must have doc comments (`#![warn(missing_docs)]`)
- No `unsafe` code (`#![forbid(unsafe_code)]`)
- No `as any` or `#[allow(clippy::all)]` without justification

## Pull Request Process

1. Create a feature branch from `main`
2. Write tests for new functionality
3. Ensure `cargo test`, `cargo clippy`, and `cargo fmt` all pass
4. Update `CHANGELOG.md` with your changes
5. Open a PR with a clear description of the change

## Module Ownership

| Module | Description |
|--------|-------------|
| `rszero/src/rest/` | Axum HTTP server wrapper |
| `rszero/src/rpc/` | Volo RPC wrapper |
| `rszero/src/config/` | Configuration management |
| `rszero/src/log/` | Structured logging |
| `rszero/src/cache/` | Redis + in-memory cache |
| `rszero/src/store/` | Database/ORM layer |
| `rszero/src/queue/` | Message queue |
| `rszero/src/limit/` | Rate limiting |
| `rszero/src/breaker/` | Circuit breaker |
| `rszero/src/discovery/` | Service discovery |
| `rszero/src/middleware/` | JWT, logging middleware |
| `rszero/src/trace/` | OpenTelemetry tracing |
| `rszero/src/concurrent/` | MapReduce, fx pipeline |
| `rszero/src/shedder/` | Load shedding |
| `rszero/src/timeout/` | Timeout enforcement |
| `rszero/src/health/` | Health checks |
| `rszero/src/metrics/` | Prometheus metrics |
| `rszero/src/error/` | Error handling |
| `rszeroctl/` | CLI scaffolding tool |

## Reporting Issues

- Use GitHub Issues for bug reports
- Include: rszero version, Rust version, OS, and a minimal reproduction
- For security issues, email security@rszero.dev instead
