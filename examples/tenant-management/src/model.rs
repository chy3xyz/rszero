//! Data models for the multi-tenant management system.
//!
//! # Tenant Isolation Model
//!
//! This example uses the **Shared Database + tenant_id column** pattern,
//! the most practical approach for SaaS applications with < 1000 tenants.
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │              SQLite Database                │
//! │  ┌─────────────────────────────────────┐    │
//! │  │ tenants                             │    │
//! │  │  id, name, status, created_at       │    │
//! │  └─────────────────────────────────────┘    │
//! │  ┌─────────────────────────────────────┐    │
//! │  │ users                               │    │
//! │  │  id, tenant_id, name, email, role   │    │
//! │  │  ↑ FOREIGN KEY                      │    │
//! │  └─────────────────────────────────────┘    │
//! └─────────────────────────────────────────────┘
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A tenant (organization) in the SaaS platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub status: TenantStatus,
    pub plan: TenantPlan,
    pub quota: TenantQuota,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tenant lifecycle status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TenantStatus {
    Active,
    Suspended,
    Pending,
}

impl TenantStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TenantStatus::Active => "active",
            TenantStatus::Suspended => "suspended",
            TenantStatus::Pending => "pending",
        }
    }
}

impl std::str::FromStr for TenantStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(TenantStatus::Active),
            "suspended" => Ok(TenantStatus::Suspended),
            "pending" => Ok(TenantStatus::Pending),
            _ => Err(format!("unknown tenant status: {}", s)),
        }
    }
}

/// Subscription plan for a tenant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TenantPlan {
    #[default]
    Free,
    Pro,
    Enterprise,
}

impl TenantPlan {
    pub fn as_str(&self) -> &'static str {
        match self {
            TenantPlan::Free => "free",
            TenantPlan::Pro => "pro",
            TenantPlan::Enterprise => "enterprise",
        }
    }
}

impl std::str::FromStr for TenantPlan {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "free" => Ok(TenantPlan::Free),
            "pro" => Ok(TenantPlan::Pro),
            "enterprise" => Ok(TenantPlan::Enterprise),
            _ => Err(format!("unknown tenant plan: {}", s)),
        }
    }
}

/// Per-tenant resource quotas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    pub max_users: i32,
    pub max_requests_per_minute: i32,
}

impl Default for TenantQuota {
    fn default() -> Self {
        Self {
            max_users: 10,
            max_requests_per_minute: 100,
        }
    }
}

/// A user belonging to a specific tenant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub tenant_id: i64,
    pub name: String,
    pub email: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// User roles within a tenant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    Member,
    Viewer,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::Member => "member",
            UserRole::Viewer => "viewer",
        }
    }
}

impl std::str::FromStr for UserRole {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(UserRole::Admin),
            "member" => Ok(UserRole::Member),
            "viewer" => Ok(UserRole::Viewer),
            _ => Err(format!("unknown user role: {}", s)),
        }
    }
}

// ─── Request / Response DTOs ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateTenantReq {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub plan: TenantPlan,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTenantReq {
    pub name: Option<String>,
    pub status: Option<TenantStatus>,
    pub plan: Option<TenantPlan>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserReq {
    pub name: String,
    pub email: String,
    #[serde(default = "default_member_role")]
    pub role: UserRole,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserReq {
    pub name: Option<String>,
    pub email: Option<String>,
    pub role: Option<UserRole>,
}

fn default_member_role() -> UserRole {
    UserRole::Member
}

/// Unified API response wrapper.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            code: 0,
            message: "success".into(),
            data: Some(data),
        }
    }
}

impl<T: Serialize> ApiResponse<T> {
    pub fn err(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn ok_empty() -> Self {
        Self {
            code: 0,
            message: "success".into(),
            data: None,
        }
    }
}
