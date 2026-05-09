//! HTTP handlers for the tenant-management API.
//!
//! Routes are grouped by access level:
//!
//! | Prefix | Middleware | Access |
//! |--------|-----------|--------|
//! | `/admin/tenants` | admin_guard | Super-admin only |
//! | `/users` | tenant_validation | Tenant-scoped (requires `X-Tenant-ID`) |
//! | `/health` | none | Public |

use axum::extract::{Extension, Path};
use axum::Json;
use std::sync::Arc;

use crate::middleware::TenantContext;
use crate::model::{
    ApiResponse, CreateTenantReq, CreateUserReq, UpdateTenantReq, UpdateUserReq,
};
use crate::service::{TenantService, UserService};

// ─── Super-Admin: Tenant Management ─────────────────────────────────────────

pub async fn list_tenants(
    Extension(svc): Extension<Arc<TenantService>>,
) -> Json<ApiResponse<Vec<crate::model::Tenant>>> {
    match svc.list().await {
        Ok(data) => Json(ApiResponse::ok(data)),
        Err(e) => Json(ApiResponse::err(500, format!("database error: {}", e))),
    }
}

pub async fn create_tenant(
    Extension(svc): Extension<Arc<TenantService>>,
    Json(req): Json<CreateTenantReq>,
) -> Json<ApiResponse<crate::model::Tenant>> {
    if req.name.is_empty() || req.slug.is_empty() {
        return Json(ApiResponse::err(400, "name and slug are required"));
    }
    match svc.create(req).await {
        Ok(data) => Json(ApiResponse::ok(data)),
        Err(e) => Json(ApiResponse::err(500, format!("failed to create tenant: {}", e))),
    }
}

pub async fn get_tenant(
    Extension(svc): Extension<Arc<TenantService>>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<crate::model::Tenant>> {
    match svc.find_by_id(id).await {
        Ok(data) => Json(ApiResponse::ok(data)),
        Err(_) => Json(ApiResponse::err(404, "tenant not found")),
    }
}

pub async fn update_tenant(
    Extension(svc): Extension<Arc<TenantService>>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateTenantReq>,
) -> Json<ApiResponse<crate::model::Tenant>> {
    match svc.update(id, req).await {
        Ok(data) => Json(ApiResponse::ok(data)),
        Err(_) => Json(ApiResponse::err(404, "tenant not found")),
    }
}

pub async fn delete_tenant(
    Extension(svc): Extension<Arc<TenantService>>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<()>> {
    match svc.delete(id).await {
        Ok(()) => Json(ApiResponse::ok_empty()),
        Err(e) => Json(ApiResponse::err(500, format!("failed to delete tenant: {}", e))),
    }
}

// ─── Tenant-Scoped: User Management ─────────────────────────────────────────

pub async fn list_users(
    Extension(svc): Extension<Arc<UserService>>,
    Extension(tenant): Extension<TenantContext>,
) -> Json<ApiResponse<Vec<crate::model::User>>> {
    match svc.list(&tenant).await {
        Ok(data) => Json(ApiResponse::ok(data)),
        Err(e) => Json(ApiResponse::err(500, format!("database error: {}", e))),
    }
}

pub async fn create_user(
    Extension(svc): Extension<Arc<UserService>>,
    Extension(tenant): Extension<TenantContext>,
    Json(req): Json<CreateUserReq>,
) -> Json<ApiResponse<crate::model::User>> {
    if req.name.is_empty() || req.email.is_empty() {
        return Json(ApiResponse::err(400, "name and email are required"));
    }
    match svc.create(&tenant, req).await {
        Ok(data) => Json(ApiResponse::ok(data)),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("quota") {
                Json(ApiResponse::err(429, msg))
            } else {
                Json(ApiResponse::err(500, format!("failed to create user: {}", msg)))
            }
        }
    }
}

pub async fn get_user(
    Extension(svc): Extension<Arc<UserService>>,
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<crate::model::User>> {
    match svc.find_by_id(&tenant, id).await {
        Ok(data) => Json(ApiResponse::ok(data)),
        Err(_) => Json(ApiResponse::err(404, "user not found")),
    }
}

pub async fn update_user(
    Extension(svc): Extension<Arc<UserService>>,
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateUserReq>,
) -> Json<ApiResponse<crate::model::User>> {
    match svc.update(&tenant, id, req).await {
        Ok(data) => Json(ApiResponse::ok(data)),
        Err(_) => Json(ApiResponse::err(404, "user not found")),
    }
}

pub async fn delete_user(
    Extension(svc): Extension<Arc<UserService>>,
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<()>> {
    match svc.delete(&tenant, id).await {
        Ok(()) => Json(ApiResponse::ok_empty()),
        Err(_) => Json(ApiResponse::err(404, "user not found")),
    }
}
