//! Database/ORM layer built on sea-orm and sqlx.
//!
//! Provides connection management, transaction support, migration helpers,
//! connection pool monitoring, automatic reconnection, and a generic CRUD repository trait.

pub mod migration;
pub mod replica;
pub mod batch;
pub mod sharding;

pub use migration::Migrator;
pub use replica::ReplicaStore;
pub use batch::{execute_batch, chunk_vec, BatchResult, DEFAULT_BATCH_SIZE};

use sea_orm::{Database, DatabaseConnection, DatabaseTransaction, TransactionTrait, ConnectionTrait, Statement};
use crate::config::StoreConfig;
use crate::error::{RszeroError, RszeroResult};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

/// Database connection wrapper with connection pool management and auto-reconnect.
#[derive(Clone)]
pub struct Store {
    conn: Arc<RwLock<DatabaseConnection>>,
    config: StoreConfig,
}

impl Store {
    /// Create a new store connection from [`StoreConfig`].
    pub async fn new(config: &StoreConfig) -> RszeroResult<Self> {
        if config.dsn.is_empty() {
            return Err(RszeroError::Database { message: "empty DSN".into(), source: None });
        }
        let conn = Self::connect_with_config(config).await?;

        tracing::info!(
            dsn = %mask_dsn(&config.dsn),
            max_connections = config.max_connections,
            "database connected"
        );

        Ok(Self {
            conn: Arc::new(RwLock::new(conn)),
            config: config.clone(),
        })
    }

    /// Internal: establish a new connection.
    async fn connect_with_config(config: &StoreConfig) -> RszeroResult<DatabaseConnection> {
        let mut opt = sea_orm::ConnectOptions::new(&config.dsn);
        opt.max_connections(config.max_connections);
        opt.min_connections(config.min_connections);
        opt.acquire_timeout(std::time::Duration::from_secs(30));
        opt.connect_timeout(std::time::Duration::from_secs(10));
        opt.idle_timeout(std::time::Duration::from_secs(600));
        opt.max_lifetime(std::time::Duration::from_secs(1800));
        opt.sqlx_logging(false);

        Database::connect(opt).await
            .map_err(|e| RszeroError::Database { message: e.to_string(), source: None })
    }

    /// Access the underlying database connection.
    pub async fn conn(&self) -> tokio::sync::RwLockReadGuard<'_, DatabaseConnection> {
        self.conn.read().await
    }

    /// Close the connection pool.
    pub async fn close(&self) -> RszeroResult<()> {
        let conn = self.conn.read().await;
        conn.clone().close().await
            .map_err(|e| RszeroError::Database { message: e.to_string(), source: None })
    }

    /// Ping the database to check connectivity.
    pub async fn ping(&self) -> RszeroResult<()> {
        let conn = self.conn.read().await;
        conn.ping().await
            .map_err(|e| RszeroError::Database { message: e.to_string(), source: None })
    }

    /// Attempt to reconnect if the connection is broken.
    pub async fn reconnect(&self) -> RszeroResult<()> {
        tracing::warn!("attempting database reconnection");
        match Self::connect_with_config(&self.config).await {
            Ok(new_conn) => {
                let mut guard = self.conn.write().await;
                *guard = new_conn;
                tracing::info!("database reconnected successfully");
                Ok(())
            }
            Err(e) => {
                tracing::error!(error = %e, "database reconnection failed");
                Err(e)
            }
        }
    }

    /// Start a background health check that automatically reconnects on failure.
    pub fn start_health_check(self, check_interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = interval(check_interval);
            loop {
                ticker.tick().await;
                if let Err(e) = self.ping().await {
                    tracing::warn!(error = %e, "database health check failed, triggering reconnect");
                    if let Err(e) = self.reconnect().await {
                        tracing::error!(error = %e, "auto-reconnect failed");
                    }
                }
            }
        })
    }

    /// Start a background health probe that reports to the given [`Health`] tracker.
    pub fn monitor_health(self, health: crate::health::Health, check_interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = interval(check_interval);
            loop {
                ticker.tick().await;
                if self.is_healthy().await {
                    health.set_dependency("database", crate::health::DependencyHealth::Healthy).await;
                } else {
                    health.set_dependency("database", crate::health::DependencyHealth::Unhealthy("ping failed".into())).await;
                    if let Err(e) = self.reconnect().await {
                        tracing::error!(error = %e, "auto-reconnect failed");
                    }
                }
            }
        })
    }

    /// Start a database transaction.
    pub async fn transaction(&self) -> RszeroResult<Transaction> {
        let conn = self.conn.read().await;
        let txn = conn.begin()
            .await
            .map_err(|e| RszeroError::Database { message: format!("begin transaction failed: {}", e), source: None })?;
        Ok(Transaction { inner: Some(txn) })
    }

    /// Execute a parameterized SQL statement safely.
    pub async fn execute(&self, sql: &str, params: Vec<sea_orm::Value>) -> RszeroResult<()> {
        let conn = self.conn.read().await;
        let backend = conn.get_database_backend();
        conn.execute(Statement::from_sql_and_values(backend, sql, params))
            .await
            .map_err(|e| RszeroError::Database { message: format!("execute failed: {}", e), source: None })?;
        Ok(())
    }

    /// Execute a raw SQL statement without parameterization.
    ///
    /// ⚠️ **Security warning**: Never pass user input into `sql`. Use
    /// [`Self::execute`] with parameters for all dynamic values.
    #[doc(hidden)]
    pub async fn execute_raw(&self, sql: &str) -> RszeroResult<()> {
        let conn = self.conn.read().await;
        let backend = conn.get_database_backend();
        conn.execute(Statement::from_string(backend, sql.to_string()))
            .await
            .map_err(|e| RszeroError::Database { message: format!("execute failed: {}", e), source: None })?;
        Ok(())
    }

    /// Get connection pool statistics.
    ///
    /// Returns configured pool size. For real-time sqlx pool metrics,
    /// instrument the pool directly or use `PgPool::size()` / `PgPool::num_idle()`.
    pub async fn stats(&self) -> PoolStats {
        PoolStats {
            size: self.config.max_connections,
            available: 0,
            acquired: 0,
        }
    }

    /// Get pool statistics using a raw sqlx query.
    ///
    /// For PostgreSQL, queries `pg_stat_activity` for active connection count.
    /// Falls back to configured values on error.
    pub async fn stats_real(&self) -> PoolStats {
        let conn = self.conn().await;
        let active: i64 = match conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Postgres,
                "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database()".to_string(),
            ))
            .await
        {
            Ok(Some(row)) => row.try_get_by_index(0).unwrap_or(0),
            _ => 0,
        };

        PoolStats {
            size: self.config.max_connections,
            available: self.config.max_connections.saturating_sub(active as u32),
            acquired: active as u32,
        }
    }

    /// Check if the connection is healthy.
    pub async fn is_healthy(&self) -> bool {
        self.ping().await.is_ok()
    }

    /// Run a health check and return detailed status.
    pub async fn health_check(&self) -> StoreHealth {
        match self.ping().await {
            Ok(()) => StoreHealth::Healthy,
            Err(e) => StoreHealth::Unhealthy(e.to_string()),
        }
    }
}

/// Active database transaction.
///
/// Automatically rolls back on drop if not committed.
pub struct Transaction {
    inner: Option<DatabaseTransaction>,
}

impl Transaction {
    /// Commit the transaction.
    pub async fn commit(mut self) -> RszeroResult<()> {
        if let Some(txn) = self.inner.take() {
            txn.commit().await
                .map_err(|e| RszeroError::Database { message: format!("commit failed: {}", e), source: None })?;
        }
        Ok(())
    }

    /// Rollback the transaction.
    pub async fn rollback(mut self) -> RszeroResult<()> {
        if let Some(txn) = self.inner.take() {
            txn.rollback().await
                .map_err(|e| RszeroError::Database { message: format!("rollback failed: {}", e), source: None })?;
        }
        Ok(())
    }

    /// Access the underlying transaction connection for queries.
    ///
    /// Returns `None` if the transaction has already been committed or rolled back.
    pub fn conn(&self) -> Option<&DatabaseTransaction> {
        self.inner.as_ref()
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        if self.inner.is_some() {
            tracing::warn!("transaction dropped without commit/rollback — rolling back");
            // Cannot async rollback in drop; sea-orm will clean up on connection close
        }
    }
}

/// Generic CRUD repository trait.
#[async_trait::async_trait]
pub trait Repository<M>: Send + Sync
where
    M: sea_orm::EntityTrait,
{
    /// Find one entity by primary key.
    async fn find_by_id(&self, db: &DatabaseConnection, id: <M::PrimaryKey as sea_orm::PrimaryKeyTrait>::ValueType) -> RszeroResult<Option<M::Model>>;

    /// Find all entities.
    async fn find_all(&self, db: &DatabaseConnection) -> RszeroResult<Vec<M::Model>>;

    /// Create a new entity.
    async fn create(&self, db: &DatabaseConnection, model: M::Model) -> RszeroResult<M::Model>;

    /// Update an entity.
    async fn update(&self, db: &DatabaseConnection, model: M::Model) -> RszeroResult<M::Model>;

    /// Delete an entity by primary key.
    async fn delete(&self, db: &DatabaseConnection, id: <M::PrimaryKey as sea_orm::PrimaryKeyTrait>::ValueType) -> RszeroResult<u64>;
}

/// Connection pool statistics.
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Total connections in the pool.
    pub size: u32,
    /// Available idle connections.
    pub available: u32,
    /// Currently acquired connections.
    pub acquired: u32,
}

/// Store health status.
#[derive(Debug, Clone)]
pub enum StoreHealth {
    /// Database is reachable.
    Healthy,
    /// Database is unreachable with error message.
    Unhealthy(String),
}

/// Mask password in DSN for logging.
fn mask_dsn(dsn: &str) -> String {
    // Simple heuristic: replace password=... with password=***
    if let Some(pos) = dsn.to_lowercase().find("password=") {
        let start = pos + "password=".len();
        let end = dsn[start..].find(&['&', ' ', '@', '/'][..]).map(|i| start + i).unwrap_or(dsn.len());
        let mut result = dsn.to_string();
        result.replace_range(start..end, "***");
        result
    } else {
        dsn.to_string()
    }
}

/// Convenience: connect to a database by DSN string.
pub async fn connect(dsn: &str) -> RszeroResult<DatabaseConnection> {
    Database::connect(dsn).await
        .map_err(|e| RszeroError::Database { message: e.to_string(), source: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_empty_dsn() {
        let config = StoreConfig::default();
        let result = Store::new(&config).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_mask_dsn() {
        let masked = mask_dsn("postgres://user:secret@localhost/db?password=mypass");
        assert!(masked.contains("***"));
        assert!(!masked.contains("mypass"));
    }

    #[test]
    fn test_store_health() {
        let health = StoreHealth::Healthy;
        assert!(matches!(health, StoreHealth::Healthy));
    }

    #[tokio::test]
    async fn test_sqlite_memory_connection() {
        let config = StoreConfig {
            dsn: "sqlite::memory:".into(),
            max_connections: 1,
            min_connections: 1,
        };

        let store = Store::new(&config).await.expect("sqlite memory connect failed");
        assert!(store.is_healthy().await);
        assert!(matches!(store.health_check().await, StoreHealth::Healthy));

        // Create a test table
        store.execute_raw(
            "CREATE TABLE test_users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT)"
        ).await.expect("create table failed");

        // Insert via parameterized query
        store.execute(
            "INSERT INTO test_users (name, email) VALUES (?, ?)",
            vec!["Alice".into(), "alice@example.com".into()],
        ).await.expect("insert failed");

        // Query back
        let conn = store.conn().await;
        let rows = conn.query_all(
            sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT name, email FROM test_users",
            )
        ).await.expect("select failed");

        assert_eq!(rows.len(), 1);
        let name: String = rows[0].try_get_by_index(0).unwrap();
        let email: String = rows[0].try_get_by_index(1).unwrap();
        assert_eq!(name, "Alice");
        assert_eq!(email, "alice@example.com");
    }

    #[tokio::test]
    async fn test_sqlite_transaction_commit() {
        let config = StoreConfig {
            dsn: "sqlite::memory:".into(),
            max_connections: 1,
            min_connections: 1,
        };

        let store = Store::new(&config).await.unwrap();
        store.execute_raw("CREATE TABLE tx_test (id INTEGER PRIMARY KEY, val TEXT)").await.unwrap();

        let txn = store.transaction().await.expect("begin transaction failed");
        let conn = txn.conn().expect("transaction connection");
        conn.execute(sea_orm::Statement::from_string(
            sea_orm::DbBackend::Sqlite,
            "INSERT INTO tx_test (val) VALUES ('committed')",
        )).await.expect("txn insert failed");

        txn.commit().await.expect("commit failed");

        let conn = store.conn().await;
        let rows = conn.query_all(
            sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT val FROM tx_test",
            )
        ).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn test_sqlite_transaction_rollback() {
        let config = StoreConfig {
            dsn: "sqlite::memory:".into(),
            max_connections: 1,
            min_connections: 1,
        };

        let store = Store::new(&config).await.unwrap();
        store.execute_raw("CREATE TABLE tx_test2 (id INTEGER PRIMARY KEY, val TEXT)").await.unwrap();

        {
            let txn = store.transaction().await.expect("begin transaction failed");
            let conn = txn.conn().expect("transaction connection");
            conn.execute(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "INSERT INTO tx_test2 (val) VALUES ('rolled-back')",
            )).await.expect("txn insert failed");
            // txn dropped without commit → rollback
        }

        let conn = store.conn().await;
        let rows = conn.query_all(
            sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT val FROM tx_test2",
            )
        ).await.unwrap();
        assert_eq!(rows.len(), 0);
    }
}
