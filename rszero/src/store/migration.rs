//! Database migration helper for managing migration files and tracking versions.

use crate::error::{RszeroError, RszeroResult};
use crate::store::Store;
use sea_orm::{ConnectionTrait, Statement, DbBackend};
use std::path::Path;

/// Migration runner for database schema changes.
pub struct Migrator {
    store: Store,
    migration_dir: String,
}

impl Migrator {
    /// Create a new migrator from a Store and migration directory.
    pub fn new(store: Store, migration_dir: &str) -> Self {
        Self {
            store,
            migration_dir: migration_dir.to_string(),
        }
    }

    /// Run all pending migrations in order.
    pub async fn run(&self) -> RszeroResult<()> {
        let path = Path::new(&self.migration_dir);
        if !path.exists() {
            tracing::warn!(dir = %self.migration_dir, "migration directory not found");
            return Ok(());
        }

        self.ensure_migration_table().await?;

        let applied = self.get_applied_migrations().await?;
        let mut files = self.get_migration_files(path).await?;
        files.sort_by(|a, b| a.name.cmp(&b.name));

        let mut count = 0;
        for file in files {
            if applied.contains(&file.name) {
                continue;
            }

            tracing::info!(migration = %file.name, "applying migration");
            let content = tokio::fs::read_to_string(&file.path).await
                .map_err(|e| RszeroError::Database { message: format!("failed to read migration {}: {}", file.name, e), source: None })?;

            self.apply_migration(&file.name, &content).await?;
            count += 1;
        }

        if count > 0 {
            tracing::info!(count, "migrations applied");
        } else {
            tracing::info!("no pending migrations");
        }

        Ok(())
    }

    /// Rollback the last applied migration.
    pub async fn rollback(&self) -> RszeroResult<()> {
        let applied = self.get_applied_migrations().await?;
        if applied.is_empty() {
            tracing::info!("no migrations to rollback");
            return Ok(());
        }

        let Some(last) = applied.last() else {
            tracing::info!("no migrations to rollback");
            return Ok(());
        };
        tracing::info!(migration = %last, "rolling back migration");

        let down_path = format!("{}/DOWN_{}", self.migration_dir, last);
        if Path::new(&down_path).exists() {
            let content = tokio::fs::read_to_string(&down_path).await
                .map_err(|e| RszeroError::Database { message: format!("failed to read rollback file: {}", e), source: None })?;

            let conn = self.store.conn().await;
            conn.execute(Statement::from_string(DbBackend::Postgres, content))
                .await
                .map_err(|e| RszeroError::Database { message: format!("rollback failed: {}", e), source: None })?;

            conn.execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "DELETE FROM schema_migrations WHERE version = $1",
                [sea_orm::Value::String(Some(Box::new(last.clone())))],
            ))
            .await
            .map_err(|e| RszeroError::Database { message: format!("failed to remove migration record: {}", e), source: None })?;

            tracing::info!(migration = %last, "rollback complete");
        } else {
            tracing::warn!(migration = %last, "no rollback file found");
        }

        Ok(())
    }

    /// Get pending migrations that haven't been applied.
    pub async fn pending(&self) -> RszeroResult<Vec<String>> {
        let path = Path::new(&self.migration_dir);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let applied = self.get_applied_migrations().await?;
        let files = self.get_migration_files(path).await?;

        Ok(files
            .into_iter()
            .filter(|f| !applied.contains(&f.name))
            .map(|f| f.name)
            .collect())
    }

    /// Get the current schema version.
    pub async fn version(&self) -> RszeroResult<Option<String>> {
        let applied = self.get_applied_migrations().await?;
        Ok(applied.last().cloned())
    }

    /// Get the migration directory path.
    pub fn migration_dir(&self) -> &str {
        &self.migration_dir
    }

    async fn ensure_migration_table(&self) -> RszeroResult<()> {
        let conn = self.store.conn().await;
        conn.execute(Statement::from_string(DbBackend::Postgres,
            r#"CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )"#.to_string(),
        ))
        .await
        .map_err(|e| RszeroError::Database { message: format!("failed to create migrations table: {}", e), source: None })?;
        Ok(())
    }

    async fn get_applied_migrations(&self) -> RszeroResult<Vec<String>> {
        let conn = self.store.conn().await;
        let result = conn.query_all(Statement::from_string(DbBackend::Postgres,
            "SELECT version FROM schema_migrations ORDER BY version".to_string(),
        ))
        .await
        .map_err(|e| RszeroError::Database { message: format!("failed to query migrations: {}", e), source: None })?;

        Ok(result
            .into_iter()
            .filter_map(|row| row.try_get_by_index::<String>(0).ok())
            .collect())
    }

    async fn get_migration_files(&self, path: &Path) -> RszeroResult<Vec<MigrationFile>> {
        let mut files = Vec::new();
        let mut entries = tokio::fs::read_dir(path).await
            .map_err(|e| RszeroError::Database { message: format!("failed to read migration dir: {}", e), source: None })?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| RszeroError::Database { message: format!("failed to read migration dir: {}", e), source: None })?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".sql") && !name.starts_with("DOWN_") {
                files.push(MigrationFile {
                    name: name.clone(),
                    path: entry.path().to_string_lossy().to_string(),
                });
            }
        }

        files.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(files)
    }

    async fn apply_migration(&self, version: &str, sql: &str) -> RszeroResult<()> {
        let conn = self.store.conn().await;
        conn.execute(Statement::from_string(DbBackend::Postgres, sql.to_string()))
            .await
            .map_err(|e| RszeroError::Database { message: format!("failed to apply migration {}: {}", version, e), source: None })?;

        conn.execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO schema_migrations (version) VALUES ($1)",
            [sea_orm::Value::String(Some(Box::new(version.to_string())))],
        ))
        .await
        .map_err(|e| RszeroError::Database { message: format!("failed to record migration: {}", e), source: None })?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MigrationFile {
    name: String,
    path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_file_sorting() {
        let mut files = [
            MigrationFile { name: "003_add_users.sql".into(), path: String::new() },
            MigrationFile { name: "001_create_schema.sql".into(), path: String::new() },
            MigrationFile { name: "002_add_indexes.sql".into(), path: String::new() },
        ];
        files.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(files[0].name, "001_create_schema.sql");
        assert_eq!(files[1].name, "002_add_indexes.sql");
        assert_eq!(files[2].name, "003_add_users.sql");
    }
}
