//! Multi-tenant middleware best practices.
//!
//! Demonstrates three critical patterns for SaaS isolation:
//!
//! 1. **Tenant Extraction** — pulls `X-Tenant-ID` from headers and injects
//!    into request extensions.
//! 2. **Tenant Validation** — verifies the tenant exists and is active
//!    *before* the handler runs (fail-fast security).
//! 3. **Tenant Rate Limiting** — per-tenant request quotas enforced at
//!    the middleware layer.

use axum::extract::{Extension, Request};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::model::ApiResponse;

/// Tenant identifier attached to every request in a multi-tenant app.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TenantContext {
    pub tenant_id: i64,
    pub tenant_slug: String,
    pub plan: String,
    pub quota_rpm: i32,
}

/// Per-tenant in-memory request counters for rate limiting.
/// In production, use Redis (e.g., rszero::Cache) for distributed rate limiting.
pub struct TenantRateLimiter {
    windows: Arc<RwLock<HashMap<i64, RateWindow>>>,
}

struct RateWindow {
    count: u32,
    reset_at: std::time::Instant,
}

impl TenantRateLimiter {
    pub fn new() -> Self {
        Self {
            windows: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if the tenant has remaining quota. Returns `true` if allowed.
    pub async fn check(&self, tenant_id: i64, quota: i32) -> bool {
        let mut windows = self.windows.write().await;
        let now = std::time::Instant::now();
        let window = windows.entry(tenant_id).or_insert_with(|| RateWindow {
            count: 0,
            reset_at: now + std::time::Duration::from_secs(60),
        });

        if now > window.reset_at {
            window.count = 0;
            window.reset_at = now + std::time::Duration::from_secs(60);
        }

        if window.count >= quota as u32 {
            return false;
        }
        window.count += 1;
        true
    }
}

/// Extract `X-Tenant-ID` header and look up the tenant in the database.
///
/// # Security Best Practice
/// Always validate the tenant at the edge (middleware) rather than in every
/// handler. This prevents accidentally leaking data for inactive/suspended
/// tenants.
pub async fn tenant_validation_middleware(
    Extension(db): Extension<Arc<Pool<Sqlite>>>,
    Extension(limiter): Extension<Arc<TenantRateLimiter>>,
    mut req: Request,
    next: Next,
) -> Response {
    // 1. Extract tenant slug from header.
    let tenant_slug = req
        .headers()
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let Some(slug) = tenant_slug else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(ApiResponse::<()>::err(400, "missing X-Tenant-ID header")),
        )
            .into_response();
    };

    // 2. Validate tenant against database.
    let row = sqlx::query_as::<_, (i64, String, String, i32)>(
        "SELECT id, slug, plan, max_rpm FROM tenants WHERE slug = ? AND status = 'active'"
    )
    .bind(&slug)
    .fetch_optional(db.as_ref())
    .await;

    let Ok(Some((id, slg, plan, rpm))) = row else {
        return (
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(ApiResponse::<()>::err(403, "tenant not found or inactive")),
        )
            .into_response();
    };

    // 3. Enforce per-tenant rate limit.
    if !limiter.check(id, rpm).await {
        return (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            axum::Json(ApiResponse::<()>::err(429, "tenant rate limit exceeded")),
        )
            .into_response();
    }

    // 4. Inject validated tenant context into extensions.
    let ctx = TenantContext {
        tenant_id: id,
        tenant_slug: slg,
        plan,
        quota_rpm: rpm,
    };
    tracing::info!(tenant = %ctx.tenant_slug, tenant_id = ctx.tenant_id, "tenant validated");
    req.extensions_mut().insert(ctx);

    next.run(req).await
}

/// Super-admin guard — checks `X-Admin-Token` header.
///
/// In production, replace with JWT validation via rszero::middleware::jwt.
pub async fn admin_guard_middleware(
    req: Request,
    next: Next,
) -> Response {
    let is_admin = req
        .headers()
        .get("x-admin-token")
        .and_then(|v| v.to_str().ok())
        == Some("supersecrettoken");

    if !is_admin {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(ApiResponse::<()>::err(401, "admin access required")),
        )
            .into_response();
    }

    next.run(req).await
}
