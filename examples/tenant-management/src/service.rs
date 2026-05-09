//! Business logic for tenant and user management.
//!
//! # Multi-Tenant Data Access Best Practices
//!
//! 1. **Always filter by `tenant_id`** — every query that touches tenant-scoped
//!    data MUST include `WHERE tenant_id = ?`. The compiler cannot enforce this,
//!    so code review and integration tests are essential.
//! 2. **Use parameterized queries** — prevents SQL injection and ensures type safety.
//! 3. **Fail fast on tenant mismatch** — if a user requests a resource ID that
//!    belongs to a different tenant, return 404 (not 403) to avoid leaking
//!    the existence of other tenants' data.
//! 4. **Check quotas before creation** — enforce plan limits in the service layer.

use sqlx::{Pool, Sqlite};

use crate::middleware::TenantContext;
use crate::model::{
    CreateTenantReq, CreateUserReq, Tenant, TenantQuota, TenantStatus, UpdateTenantReq,
    UpdateUserReq, User, UserRole,
};

/// Tenant administration service.
///
/// These operations are **cross-tenant** and should only be accessible by
/// the platform operator (super-admin).
pub struct TenantService {
    db: Pool<Sqlite>,
}

impl TenantService {
    pub fn new(db: Pool<Sqlite>) -> Self {
        Self { db }
    }

    pub async fn create(&self, req: CreateTenantReq) -> anyhow::Result<Tenant> {
        let quota = TenantQuota::default();
        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO tenants (name, slug, status, plan, max_users, max_rpm)
            VALUES (?, ?, ?, ?, ?, ?)
            RETURNING id
            "#,
        )
        .bind(&req.name)
        .bind(&req.slug)
        .bind(TenantStatus::Active.as_str())
        .bind(req.plan.as_str())
        .bind(quota.max_users)
        .bind(quota.max_requests_per_minute)
        .fetch_one(&self.db)
        .await?;

        self.find_by_id(id).await
    }

    pub async fn list(&self) -> anyhow::Result<Vec<Tenant>> {
        let rows = sqlx::query_as::<_, TenantRow>(
            "SELECT id, name, slug, status, plan, max_users, max_rpm, created_at, updated_at FROM tenants ORDER BY id"
        )
        .fetch_all(&self.db)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn find_by_id(&self, id: i64) -> anyhow::Result<Tenant> {
        let row = sqlx::query_as::<_, TenantRow>(
            "SELECT id, name, slug, status, plan, max_users, max_rpm, created_at, updated_at FROM tenants WHERE id = ?"
        )
        .bind(id)
        .fetch_one(&self.db)
        .await?;

        Ok(row.into())
    }

    pub async fn update(&self, id: i64, req: UpdateTenantReq) -> anyhow::Result<Tenant> {
        if let Some(name) = &req.name {
            sqlx::query("UPDATE tenants SET name = ? WHERE id = ?")
                .bind(name)
                .bind(id)
                .execute(&self.db)
                .await?;
        }
        if let Some(status) = req.status {
            sqlx::query("UPDATE tenants SET status = ? WHERE id = ?")
                .bind(status.as_str())
                .bind(id)
                .execute(&self.db)
                .await?;
        }
        if let Some(plan) = req.plan {
            sqlx::query("UPDATE tenants SET plan = ? WHERE id = ?")
                .bind(plan.as_str())
                .bind(id)
                .execute(&self.db)
                .await?;
        }

        self.find_by_id(id).await
    }

    pub async fn delete(&self, id: i64) -> anyhow::Result<()> {
        // Soft-delete by marking suspended. Hard delete cascades to users.
        sqlx::query("UPDATE tenants SET status = 'suspended' WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(())
    }
}

/// User service scoped to a single tenant.
///
/// **CRITICAL**: Every method in this struct receives `&TenantContext` and
/// injects `tenant_id` into the WHERE clause. This is the isolation boundary.
pub struct UserService {
    db: Pool<Sqlite>,
}

impl UserService {
    pub fn new(db: Pool<Sqlite>) -> Self {
        Self { db }
    }

    pub async fn create(
        &self,
        tenant: &TenantContext,
        req: CreateUserReq,
    ) -> anyhow::Result<User> {
        // Enforce tenant user quota.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id = ?")
            .bind(tenant.tenant_id)
            .fetch_one(&self.db)
            .await?;

        let max_users: i32 = sqlx::query_scalar("SELECT max_users FROM tenants WHERE id = ?")
            .bind(tenant.tenant_id)
            .fetch_one(&self.db)
            .await?;

        if count >= max_users as i64 {
            anyhow::bail!("tenant user quota exceeded: {} of {}", count, max_users);
        }

        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (tenant_id, name, email, role) VALUES (?, ?, ?, ?) RETURNING id"
        )
        .bind(tenant.tenant_id)
        .bind(&req.name)
        .bind(&req.email)
        .bind(req.role.as_str())
        .fetch_one(&self.db)
        .await?;

        self.find_by_id(tenant, id).await
    }

    pub async fn list(&self, tenant: &TenantContext) -> anyhow::Result<Vec<User>> {
        let rows = sqlx::query_as::<_, UserRow>(
            "SELECT id, tenant_id, name, email, role, created_at, updated_at FROM users WHERE tenant_id = ? ORDER BY id"
        )
        .bind(tenant.tenant_id)
        .fetch_all(&self.db)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn find_by_id(
        &self,
        tenant: &TenantContext,
        id: i64,
    ) -> anyhow::Result<User> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, tenant_id, name, email, role, created_at, updated_at FROM users WHERE id = ? AND tenant_id = ?"
        )
        .bind(id)
        .bind(tenant.tenant_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| anyhow::anyhow!("user not found"))?;

        Ok(row.into())
    }

    pub async fn update(
        &self,
        tenant: &TenantContext,
        id: i64,
        req: UpdateUserReq,
    ) -> anyhow::Result<User> {
        // Verify ownership before updating (prevents cross-tenant access by ID enumeration).
        let _ = self.find_by_id(tenant, id).await?;

        if let Some(name) = &req.name {
            sqlx::query("UPDATE users SET name = ? WHERE id = ? AND tenant_id = ?")
                .bind(name)
                .bind(id)
                .bind(tenant.tenant_id)
                .execute(&self.db)
                .await?;
        }
        if let Some(email) = &req.email {
            sqlx::query("UPDATE users SET email = ? WHERE id = ? AND tenant_id = ?")
                .bind(email)
                .bind(id)
                .bind(tenant.tenant_id)
                .execute(&self.db)
                .await?;
        }
        if let Some(role) = req.role {
            sqlx::query("UPDATE users SET role = ? WHERE id = ? AND tenant_id = ?")
                .bind(role.as_str())
                .bind(id)
                .bind(tenant.tenant_id)
                .execute(&self.db)
                .await?;
        }

        self.find_by_id(tenant, id).await
    }

    pub async fn delete(&self, tenant: &TenantContext, id: i64) -> anyhow::Result<()> {
        let result = sqlx::query("DELETE FROM users WHERE id = ? AND tenant_id = ?")
            .bind(id)
            .bind(tenant.tenant_id)
            .execute(&self.db)
            .await?;

        if result.rows_affected() == 0 {
            anyhow::bail!("user not found");
        }
        Ok(())
    }
}

// ─── Database Row Mappings ──────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct TenantRow {
    id: i64,
    name: String,
    slug: String,
    status: String,
    plan: String,
    max_users: i32,
    max_rpm: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<TenantRow> for Tenant {
    fn from(r: TenantRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            slug: r.slug,
            status: r.status.parse().unwrap_or(TenantStatus::Pending),
            plan: r.plan.parse().unwrap_or(crate::model::TenantPlan::Free),
            quota: TenantQuota {
                max_users: r.max_users,
                max_requests_per_minute: r.max_rpm,
            },
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: i64,
    tenant_id: i64,
    name: String,
    email: String,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<UserRow> for User {
    fn from(r: UserRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            email: r.email,
            role: r.role.parse().unwrap_or(UserRole::Member),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}
