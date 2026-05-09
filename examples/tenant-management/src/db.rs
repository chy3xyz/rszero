//! Database setup for the tenant-management example.
//!
//! Uses SQLite (in-memory for demo) with sqlx migrations.
//! In production, switch to PostgreSQL and replace `:memory:` with a real DSN.

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Pool, Sqlite};
use std::time::Duration;

/// Initialize the SQLite database and run migrations.
pub async fn init_db() -> anyhow::Result<Pool<Sqlite>> {
    // In-memory database — fast, zero external dependencies for the example.
    // For production, use: "postgres://user:pass@localhost/tenant_db"
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect("sqlite::memory:")
        .await?;

    // Run schema migrations inline (no external migration files needed for the demo).
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tenants (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            slug        TEXT NOT NULL UNIQUE,
            status      TEXT NOT NULL DEFAULT 'pending',
            plan        TEXT NOT NULL DEFAULT 'free',
            max_users   INTEGER NOT NULL DEFAULT 10,
            max_rpm     INTEGER NOT NULL DEFAULT 100,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX idx_tenants_slug ON tenants(slug);
        CREATE INDEX idx_tenants_status ON tenants(status);

        CREATE TABLE IF NOT EXISTS users (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            tenant_id   INTEGER NOT NULL,
            name        TEXT NOT NULL,
            email       TEXT NOT NULL,
            role        TEXT NOT NULL DEFAULT 'member',
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_users_tenant ON users(tenant_id);
        CREATE UNIQUE INDEX idx_users_email_per_tenant ON users(tenant_id, email);
        "#,
    )
    .execute(&pool)
    .await?;

    // Seed a default tenant and an admin user for immediate testing.
    let tenant_id: i64 = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug, status, plan) VALUES (?, ?, ?, ?) RETURNING id"
    )
    .bind("Acme Corporation")
    .bind("acme")
    .bind("active")
    .bind("enterprise")
    .fetch_one(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO users (tenant_id, name, email, role) VALUES (?, ?, ?, ?)"
    )
    .bind(tenant_id)
    .bind("Alice Admin")
    .bind("alice@acme.com")
    .bind("admin")
    .execute(&pool)
    .await?;

    tracing::info!(tenant_id, "database initialized with seed data");
    Ok(pool)
}
