//! Multi-Tenant Management System — rszero Best Practices Example
//!
//! Demonstrates production-grade patterns for SaaS applications:
//!
//! - **Tenant Isolation**: shared database with `tenant_id` column filtering
//! - **Edge Validation**: tenant existence & status checked in middleware
//! - **Per-Tenant Rate Limiting**: quota enforcement per tenant at the edge
//! - **Plan-Based Quotas**: user limits and RPM tied to tenant subscription
//! - **Fail-Fast Security**: 404 on cross-tenant ID enumeration (no data leakage)
//! - **Admin Guard**: super-admin endpoints protected by token header
//!
//! # Run
//!
//! ```bash
//! cd examples/tenant-management
//! cargo run
//! ```
//!
//! # Test
//!
//! ```bash
//! # 1. Health check (public)
//! curl http://localhost:8080/health
//!
//! # 2. List tenants (requires admin token)
//! curl -H "X-Admin-Token: supersecrettoken" http://localhost:8080/admin/tenants
//!
//! # 3. Create a tenant (admin)
//! curl -X POST -H "X-Admin-Token: supersecrettoken" \
//!   -H "Content-Type: application/json" \
//!   -d '{"name":"Beta Corp","slug":"beta","plan":"pro"}' \
//!   http://localhost:8080/admin/tenants
//!
//! # 4. List users in the default tenant (acme)
//! curl -H "X-Tenant-ID: acme" http://localhost:8080/users
//!
//! # 5. Create a user in the default tenant
//! curl -X POST -H "X-Tenant-ID: acme" -H "Content-Type: application/json" \
//!   -d '{"name":"Bob Member","email":"bob@acme.com","role":"member"}' \
//!   http://localhost:8080/users
//!
//! # 6. Try accessing without tenant header (should fail)
//! curl http://localhost:8080/users
//! # => {"code":400,"message":"missing X-Tenant-ID header"}
//!
//! # 7. Try accessing with invalid tenant (should fail)
//! curl -H "X-Tenant-ID: evilcorp" http://localhost:8080/users
//! # => {"code":403,"message":"tenant not found or inactive"}
//!
//! # 8. Try cross-tenant user access (should 404, not 403 — no leakage)
//! curl -H "X-Tenant-ID: beta" http://localhost:8080/users/1
//! # => {"code":404,"message":"user not found"}
//! ```

use std::sync::Arc;

use axum::routing::get;
use axum::Extension;
use axum::Router;
use sqlx::Pool;
use sqlx::Sqlite;

mod db;
mod handler;
mod middleware;
mod model;
mod service;

use middleware::{admin_guard_middleware, tenant_validation_middleware, TenantRateLimiter};
use service::{TenantService, UserService};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing for structured logs.
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .init();

    tracing::info!("starting tenant-management service");

    // 1. Initialize SQLite in-memory database with seed data.
    let db = db::init_db().await?;
    let db = Arc::new(db);

    // 2. Create service layer instances.
    let tenant_svc = Arc::new(TenantService::new((*db).clone()));
    let user_svc = Arc::new(UserService::new((*db).clone()));

    // 3. Shared per-tenant rate limiter.
    let rate_limiter = Arc::new(TenantRateLimiter::new());

    // 4. Build router with middleware layers.
    let app = Router::new()
        // Public health endpoints.
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        // Super-admin routes: no tenant validation, but require admin token.
        .route("/admin/tenants", get(handler::list_tenants).post(handler::create_tenant))
        .route(
            "/admin/tenants/:id",
            get(handler::get_tenant)
                .put(handler::update_tenant)
                .delete(handler::delete_tenant),
        )
        .layer(axum::middleware::from_fn(admin_guard_middleware))
        // Tenant-scoped routes: validate tenant + rate limit.
        .route("/users", get(handler::list_users).post(handler::create_user))
        .route(
            "/users/:id",
            get(handler::get_user)
                .put(handler::update_user)
                .delete(handler::delete_user),
        )
        .layer(axum::middleware::from_fn(tenant_validation_middleware))
        .layer(Extension(db.clone()))
        .layer(Extension(rate_limiter.clone()))
        // Attach shared state.
        .layer(axum::Extension(tenant_svc.clone()))
        .layer(axum::Extension(user_svc.clone()));

    // 5. Start server.
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("listening on http://0.0.0.0:8080");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_handler() -> axum::Json<model::ApiResponse<()>> {
    axum::Json(model::ApiResponse::ok_empty())
}

async fn ready_handler(
    Extension(db): Extension<Arc<Pool<Sqlite>>>,
) -> axum::Json<model::ApiResponse<()>> {
    match sqlx::query("SELECT 1").execute(db.as_ref()).await {
        Ok(_) => axum::Json(model::ApiResponse::ok_empty()),
        Err(e) => axum::Json(model::ApiResponse::err(503, format!("not ready: {}", e))),
    }
}
