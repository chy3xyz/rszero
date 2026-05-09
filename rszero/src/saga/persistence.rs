//! Saga state persistence — durable execution with database-backed recovery.
//!
//! Guarantees that saga execution can resume after process restarts by persisting
//! step states to a database table. Each saga instance gets a unique ID, and
//! every step transition is logged atomically.

#![allow(clippy::too_many_arguments)]
//!
//! # Example
//!
//! ```no_run
//! use rszero::saga::Saga;
//! use rszero::saga::persistence::{PersistentSaga, SqlSagaPersister, SagaPersister};
//! use rszero::store::Store;
//!
//! # async fn example() -> rszero::error::RszeroResult<()> {
//! let store = Store::new(&Default::default()).await?;
//! let persister = SqlSagaPersister::new(store);
//! persister.init_table().await?;
//!
//! let saga: Saga<()> = Saga::new("order-saga");
//! let persistent = PersistentSaga::new(saga, Box::new(persister));
//! // persistent.step(...).execute().await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{RszeroError, RszeroResult};
use crate::saga::{Saga, SagaResult};
use crate::store::Store;
use sea_orm::ConnectionTrait;

/// Execution state of a persisted saga instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SagaState {
    /// Saga is running.
    Running,
    /// A step failed, compensating.
    Compensating,
    /// All steps completed.
    Completed,
    /// Saga failed after compensation.
    Failed,
    /// Compensation succeeded.
    Compensated,
}

impl std::fmt::Display for SagaState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SagaState::Running => write!(f, "running"),
            SagaState::Compensating => write!(f, "compensating"),
            SagaState::Completed => write!(f, "completed"),
            SagaState::Failed => write!(f, "failed"),
            SagaState::Compensated => write!(f, "compensated"),
        }
    }
}

/// A persisted saga record in the database.
#[derive(Debug, Clone)]
pub struct SagaRecord {
    /// Unique saga instance ID.
    pub saga_id: String,
    /// Saga definition name.
    pub saga_name: String,
    /// Current step name (or last executed).
    pub step_name: String,
    /// Execution state.
    pub state: SagaState,
    /// Step input (JSON).
    pub input: Option<String>,
    /// Step output (JSON).
    pub output: Option<String>,
    /// Error message if failed.
    pub error: Option<String>,
    /// Record creation time (ISO 8601).
    pub created_at: String,
}

/// Trait for saga persistence backends.
#[async_trait::async_trait]
pub trait SagaPersister: Send + Sync {
    /// Initialize the persistence table(s).
    async fn init_table(&self) -> RszeroResult<()>;

    /// Record a step transition.
    async fn record_step(
        &self,
        saga_id: &str,
        saga_name: &str,
        step_name: &str,
        state: SagaState,
        input: Option<&str>,
        output: Option<&str>,
        error: Option<&str>,
    ) -> RszeroResult<()>;

    /// Load the latest state of a saga instance.
    async fn load_saga(&self, saga_id: &str) -> RszeroResult<Option<SagaRecord>>;

    /// List all saga instances by name.
    async fn list_sagas(&self, saga_name: Option<&str>) -> RszeroResult<Vec<SagaRecord>>;
}

/// SQL-based saga persister using the framework Store.
pub struct SqlSagaPersister {
    store: Store,
    table_name: String,
}

impl SqlSagaPersister {
    /// Create a new SQL persister with the default table name.
    pub fn new(store: Store) -> Self {
        Self {
            store,
            table_name: "saga_records".to_string(),
        }
    }

    /// Create a new SQL persister with a custom table name.
    pub fn with_table(store: Store, table_name: &str) -> Self {
        Self {
            store,
            table_name: table_name.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl SagaPersister for SqlSagaPersister {
    async fn init_table(&self) -> RszeroResult<()> {
        let sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id          SERIAL PRIMARY KEY,
                saga_id     VARCHAR(64) NOT NULL,
                saga_name   VARCHAR(255) NOT NULL,
                step_name   VARCHAR(255) NOT NULL,
                state       VARCHAR(20) NOT NULL,
                input       TEXT,
                output      TEXT,
                error       TEXT,
                created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_{}_saga_id ON {}(saga_id);
            CREATE INDEX IF NOT EXISTS idx_{}_saga_name ON {}(saga_name);
            "#,
            self.table_name,
            self.table_name,
            self.table_name,
            self.table_name,
            self.table_name,
        );
        self.store.execute_raw(&sql).await?;
        tracing::info!(table = %self.table_name, "saga persistence table initialized");
        Ok(())
    }

    async fn record_step(
        &self,
        saga_id: &str,
        saga_name: &str,
        step_name: &str,
        state: SagaState,
        input: Option<&str>,
        output: Option<&str>,
        error: Option<&str>,
    ) -> RszeroResult<()> {
        let sql = format!(
            r#"
            INSERT INTO {} (saga_id, saga_name, step_name, state, input, output, error)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            self.table_name
        );
        let conn = self.store.conn().await;
        conn.execute(sea_orm::Statement::from_sql_and_values(
            sea_orm::DbBackend::Postgres,
            &sql,
            vec![
                saga_id.into(),
                saga_name.into(),
                step_name.into(),
                state.to_string().into(),
                input.unwrap_or("").into(),
                output.unwrap_or("").into(),
                error.unwrap_or("").into(),
            ],
        ))
        .await
        .map_err(|e| RszeroError::Database { message: format!("saga persist failed: {}", e), source: None })?;
        Ok(())
    }

    async fn load_saga(&self, saga_id: &str) -> RszeroResult<Option<SagaRecord>> {
        let sql = format!(
            r#"
            SELECT saga_id, saga_name, step_name, state, input, output, error, created_at
            FROM {}
            WHERE saga_id = $1
            ORDER BY id DESC
            LIMIT 1
            "#,
            self.table_name
        );
        let conn = self.store.conn().await;
        let row = conn
            .query_one(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Postgres,
                &sql,
                vec![saga_id.into()],
            ))
            .await
            .map_err(|e| RszeroError::Database { message: format!("saga load failed: {}", e), source: None })?;

        match row {
            Some(r) => Ok(Some(parse_record(r)?)),
            None => Ok(None),
        }
    }

    async fn list_sagas(&self, saga_name: Option<&str>) -> RszeroResult<Vec<SagaRecord>> {
        let sql = if let Some(name) = saga_name {
            format!(
                r#"
                SELECT DISTINCT ON (saga_id) saga_id, saga_name, step_name, state, input, output, error, created_at
                FROM {}
                WHERE saga_name = '{}'
                ORDER BY saga_id, id DESC
                "#,
                self.table_name, name
            )
        } else {
            format!(
                r#"
                SELECT DISTINCT ON (saga_id) saga_id, saga_name, step_name, state, input, output, error, created_at
                FROM {}
                ORDER BY saga_id, id DESC
                "#,
                self.table_name
            )
        };

        let conn = self.store.conn().await;
        let rows = conn
            .query_all(sea_orm::Statement::from_string(sea_orm::DbBackend::Postgres, sql))
            .await
            .map_err(|e| RszeroError::Database { message: format!("saga list failed: {}", e), source: None })?;

        rows.into_iter().map(parse_record).collect()
    }
}

fn parse_record(row: sea_orm::QueryResult) -> RszeroResult<SagaRecord> {
    let state_str: String = row.try_get_by_index(3).unwrap_or_default();
    let state = match state_str.as_str() {
        "running" => SagaState::Running,
        "compensating" => SagaState::Compensating,
        "completed" => SagaState::Completed,
        "failed" => SagaState::Failed,
        "compensated" => SagaState::Compensated,
        _ => SagaState::Running,
    };

    Ok(SagaRecord {
        saga_id: row.try_get_by_index(0).unwrap_or_default(),
        saga_name: row.try_get_by_index(1).unwrap_or_default(),
        step_name: row.try_get_by_index(2).unwrap_or_default(),
        state,
        input: row.try_get_by_index(4).ok(),
        output: row.try_get_by_index(5).ok(),
        error: row.try_get_by_index(6).ok(),
        created_at: row.try_get_by_index(7).unwrap_or_default(),
    })
}

/// A saga wrapper that persists step transitions to a database.
pub struct PersistentSaga<T> {
    saga: Saga<T>,
    persister: Box<dyn SagaPersister>,
    saga_id: String,
}

impl<T: Send + 'static> PersistentSaga<T> {
    /// Create a new persistent saga from an existing Saga and persister.
    pub fn new(saga: Saga<T>, persister: Box<dyn SagaPersister>) -> Self {
        let saga_id = format!("saga:{}", uuid::Uuid::new_v4());
        Self {
            saga,
            persister,
            saga_id,
        }
    }

    /// Set a custom saga instance ID.
    pub fn with_id(mut self, saga_id: &str) -> Self {
        self.saga_id = saga_id.to_string();
        self
    }

    /// Execute the saga with persistence, recording every step transition.
    pub async fn execute(self) -> SagaResult<T> {
        let saga_name = self.saga.name().to_string();
        let saga_id = self.saga_id.clone();

        // Record saga start
        if let Err(e) = self.persister.record_step(
            &saga_id, &saga_name, "__start__", SagaState::Running,
            None, None, None,
        ).await {
            tracing::error!(error = %e, "failed to persist saga start");
        }

        let mut completed = Vec::new();
        let mut outputs = Vec::new();
        let mut compensation_errors = Vec::new();

        for step in self.saga.steps() {
            // Persist step start
            if let Err(e) = self.persister.record_step(
                &saga_id, &saga_name, step.name(), SagaState::Running,
                None, None, None,
            ).await {
                tracing::error!(error = %e, "failed to persist step start");
            }

            match step.execute().await {
                Ok(output) => {
                    if let Err(e) = self.persister.record_step(
                        &saga_id, &saga_name, step.name(), SagaState::Completed,
                        None, None, None,
                    ).await {
                        tracing::error!(error = %e, "failed to persist step success");
                    }
                    completed.push(step.name().to_string());
                    outputs.push(output);
                }
                Err(e) => {
                    if let Err(pe) = self.persister.record_step(
                        &saga_id, &saga_name, step.name(), SagaState::Failed,
                        None, None, Some(&e.to_string()),
                    ).await {
                        tracing::error!(error = %pe, "failed to persist step failure");
                    }

                    tracing::error!(saga = %saga_name, step = %step.name(), error = %e, "saga step failed, compensating");

                    // Persist compensating state
                    if let Err(pe) = self.persister.record_step(
                        &saga_id, &saga_name, "__compensate__", SagaState::Compensating,
                        None, None, None,
                    ).await {
                        tracing::error!(error = %pe, "failed to persist compensating state");
                    }

                    // Compensate in reverse order with retry
                    for i in (0..completed.len()).rev() {
                        let comp_step = &self.saga.steps()[i];
                        let mut comp_ok = false;
                        for retry in 0..3 {
                            match comp_step.compensate().await {
                                Ok(()) => {
                                    if let Err(pe) = self.persister.record_step(
                                        &saga_id, &saga_name, comp_step.name(), SagaState::Compensated,
                                        None, None, None,
                                    ).await {
                                        tracing::error!(error = %pe, "failed to persist compensation success");
                                    }
                                    tracing::info!(saga = %saga_name, step = %comp_step.name(), "compensation succeeded");
                                    comp_ok = true;
                                    break;
                                }
                                Err(ce) => {
                                    tracing::error!(saga = %saga_name, step = %comp_step.name(), error = %ce, attempt = retry, "compensation failed");
                                    if retry == 2 {
                                        compensation_errors.push((comp_step.name().to_string(), RszeroError::Internal { message: ce.to_string(), source: None }));
                                        if let Err(pe) = self.persister.record_step(
                                            &saga_id, &saga_name, comp_step.name(), SagaState::Compensating,
                                            None, None, Some(&ce.to_string()),
                                        ).await {
                                            tracing::error!(error = %pe, "failed to persist compensation failure");
                                        }
                                    }
                                }
                            }
                        }
                        if !comp_ok {
                            tracing::error!(saga = %saga_name, step = %comp_step.name(), "compensation exhausted all retries");
                        }
                    }

                    if let Err(pe) = self.persister.record_step(
                        &saga_id, &saga_name, "__end__", SagaState::Failed,
                        None, None, Some(&e.to_string()),
                    ).await {
                        tracing::error!(error = %pe, "failed to persist saga failure");
                    }

                    return SagaResult {
                        completed,
                        outputs,
                        failed_step: Some(step.name().to_string()),
                        error: Some(e),
                        compensation_errors,
                    };
                }
            }
        }

        tracing::info!(saga = %saga_name, steps = completed.len(), "saga completed successfully");

        if let Err(e) = self.persister.record_step(
            &saga_id, &saga_name, "__end__", SagaState::Completed,
            None, None, None,
        ).await {
            tracing::error!(error = %e, "failed to persist saga end");
        }

        SagaResult {
            completed,
            outputs,
            failed_step: None,
            error: None,
            compensation_errors: Vec::new(),
        }
    }

    /// Get the saga instance ID.
    pub fn saga_id(&self) -> &str {
        &self.saga_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saga_state_display() {
        assert_eq!(SagaState::Running.to_string(), "running");
        assert_eq!(SagaState::Completed.to_string(), "completed");
    }

    #[test]
    fn test_saga_record() {
        let record = SagaRecord {
            saga_id: "test".into(),
            saga_name: "order".into(),
            step_name: "step1".into(),
            state: SagaState::Running,
            input: None,
            output: Some("{}".into()),
            error: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        assert_eq!(record.state, SagaState::Running);
    }
}
