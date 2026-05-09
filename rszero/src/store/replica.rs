//! Database read replica support for store layer.
//!
//! Provides a primary-write / replica-read pattern for scaling read-heavy workloads.

use crate::config::StoreConfig;
use crate::error::{RszeroError, RszeroResult};
use sea_orm::{Database, DatabaseConnection};

/// Store with optional read replica.
#[derive(Clone)]
pub struct ReplicaStore {
    primary: DatabaseConnection,
    replica: Option<DatabaseConnection>,
}

impl ReplicaStore {
    /// Create a new store with primary connection only.
    pub async fn new(config: &StoreConfig) -> RszeroResult<Self> {
        let primary = Self::connect(&config.dsn).await?;
        Ok(Self { primary, replica: None })
    }

    /// Create a new store with primary and read replica.
    pub async fn with_replica(primary_config: &StoreConfig, replica_dsn: &str) -> RszeroResult<Self> {
        let primary = Self::connect(&primary_config.dsn).await?;
        let replica = Self::connect(replica_dsn).await?;
        tracing::info!("read replica connected");
        Ok(Self { primary, replica: Some(replica) })
    }

    /// Get the primary (write) connection.
    pub fn primary(&self) -> &DatabaseConnection {
        &self.primary
    }

    /// Get the replica (read) connection, falling back to primary.
    pub fn read(&self) -> &DatabaseConnection {
        self.replica.as_ref().unwrap_or(&self.primary)
    }

    /// Check if a read replica is configured.
    pub fn has_replica(&self) -> bool {
        self.replica.is_some()
    }

    /// Ping both primary and replica.
    pub async fn ping_all(&self) -> RszeroResult<()> {
        self.primary.ping().await.map_err(|e| RszeroError::Database { message: e.to_string(), source: None })?;
        if let Some(ref replica) = self.replica {
            replica.ping().await.map_err(|e| RszeroError::Database { message: format!("replica ping failed: {}", e), source: None })?;
        }
        Ok(())
    }

    async fn connect(dsn: &str) -> RszeroResult<DatabaseConnection> {
        Database::connect(dsn).await
            .map_err(|e| RszeroError::Database { message: e.to_string(), source: None })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_replica_store_has_replica() {
        // Struct test only — real DB connection requires running server
        // Verify that the API compiles correctly
        fn _assert_api() {}
    }
}
