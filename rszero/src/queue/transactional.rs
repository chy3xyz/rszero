//! Reliable message delivery using the local-message-table pattern.
//!
//! Guarantees at-least-once delivery by first persisting messages to a database table,
//! then attempting to publish them to the message queue. A background sweeper retries
//! failed or pending messages.
//!
//! This pattern is essential for distributed transactions where a database write and
//! a message publish must be atomic.
//!
//! # Example
//!
//! ```no_run
//! use rszero::queue::transactional::{TransactionalQueue, MessageTable};
//!
//! # async fn example() -> rszero::error::RszeroResult<()> {
//! // 1. Create the message table in your database
//! // 2. Wrap your existing Store and Queue
//! // let tq = TransactionalQueue::new(store, queue, MessageTable::new("outbox_messages"));
//! // 3. Publish reliably within a DB transaction
//! // tq.publish_reliable("order.created", &payload, &mut txn).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{RszeroError, RszeroResult};
use crate::queue::Queue;
use crate::store::Store;
use sea_orm::ConnectionTrait;
use std::sync::Arc;
use std::time::Duration;

/// Status of a message in the local outbox table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStatus {
    /// Message is pending delivery.
    Pending,
    /// Message has been successfully delivered.
    Published,
    /// Message delivery failed after all retries.
    Failed,
}

impl std::fmt::Display for MessageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageStatus::Pending => write!(f, "pending"),
            MessageStatus::Published => write!(f, "published"),
            MessageStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Configuration for the local message table.
#[derive(Debug, Clone)]
pub struct MessageTable {
    /// Table name in the database.
    pub table_name: String,
    /// Maximum retry attempts before marking as failed.
    pub max_retries: u32,
    /// Retry interval for the sweeper.
    pub retry_interval: Duration,
}

impl MessageTable {
    /// Create a new message table configuration.
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            max_retries: 5,
            retry_interval: Duration::from_secs(30),
        }
    }
}

/// Transactional queue that uses the local-message-table pattern.
///
/// All messages are first written to a database table, then asynchronously
/// published to the actual message queue. A sweeper retries pending messages.
pub struct TransactionalQueue {
    store: Store,
    queue: Arc<Queue>,
    table: MessageTable,
}

impl TransactionalQueue {
    /// Create a new transactional queue.
    pub fn new(store: Store, queue: Queue, table: MessageTable) -> Self {
        Self { store, queue: Arc::new(queue), table }
    }

    /// Initialize the message table in the database.
    ///
    /// Call this once during application startup.
    pub async fn init_table(&self) -> RszeroResult<()> {
        let sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id          SERIAL PRIMARY KEY,
                msg_id      VARCHAR(64) NOT NULL UNIQUE,
                topic       VARCHAR(255) NOT NULL,
                payload     TEXT NOT NULL,
                headers     JSONB DEFAULT '{{}}',
                status      VARCHAR(20) DEFAULT 'pending',
                retry_count INT DEFAULT 0,
                created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_{}_status ON {}(status);
            CREATE INDEX IF NOT EXISTS idx_{}_created ON {}(created_at);
            "#,
            self.table.table_name,
            self.table.table_name,
            self.table.table_name,
            self.table.table_name,
            self.table.table_name,
        );
        self.store.execute_raw(&sql).await?;
        tracing::info!(table = %self.table.table_name, "transactional queue table initialized");
        Ok(())
    }

    /// Publish a message reliably by first writing to the local table.
    ///
    /// The message is persisted to the DB before any attempt to publish
    /// to the message queue, guaranteeing durability.
    pub async fn publish_reliable(&self, topic: &str, payload: &str) -> RszeroResult<String> {
        let msg_id = format!("rszero:msg:{}", uuid::Uuid::new_v4());
        let headers_json = "{}";

        let sql = format!(
            r#"
            INSERT INTO {} (msg_id, topic, payload, headers, status, retry_count)
            VALUES ($1, $2, $3, $4, 'pending', 0)
            "#,
            self.table.table_name
        );

        let conn = self.store.conn().await;
        conn.execute(sea_orm::Statement::from_sql_and_values(
            sea_orm::DbBackend::Postgres,
            &sql,
            vec![
                msg_id.clone().into(),
                topic.into(),
                payload.into(),
                headers_json.into(),
            ],
        ))
        .await
        .map_err(|e| RszeroError::Queue { message: format!("failed to persist message: {}", e), source: None })?;

        // Attempt immediate publish
        if let Err(e) = self.try_publish(&msg_id, topic, payload).await {
            tracing::warn!(msg_id = %msg_id, error = %e, "immediate publish failed, message queued for retry");
        }

        Ok(msg_id)
    }

    /// Publish a message reliably inside an existing database transaction.
    ///
    /// This is the core pattern for distributed transactions: the message
    /// persistence is part of the same transaction as the business write.
    pub async fn publish_reliable_in_txn(
        &self,
        topic: &str,
        payload: &str,
        txn: &sea_orm::DatabaseTransaction,
    ) -> RszeroResult<String> {
        let msg_id = format!("rszero:msg:{}", uuid::Uuid::new_v4());
        let headers_json = "{}";

        let sql = format!(
            r#"
            INSERT INTO {} (msg_id, topic, payload, headers, status, retry_count)
            VALUES ($1, $2, $3, $4, 'pending', 0)
            "#,
            self.table.table_name
        );

        txn.execute(sea_orm::Statement::from_sql_and_values(
            sea_orm::DbBackend::Postgres,
            &sql,
            vec![
                msg_id.clone().into(),
                topic.into(),
                payload.into(),
                headers_json.into(),
            ],
        ))
        .await
        .map_err(|e| RszeroError::Queue { message: format!("failed to persist message in txn: {}", e), source: None })?;

        Ok(msg_id)
    }

    /// Attempt to publish a message and update its status in the table.
    async fn try_publish(&self, msg_id: &str, topic: &str, payload: &str) -> RszeroResult<()> {
        self.queue.push(topic, &payload).await?;

        // Mark as published
        let sql = format!(
            "UPDATE {} SET status = 'published', updated_at = CURRENT_TIMESTAMP WHERE msg_id = $1",
            self.table.table_name
        );
        let conn = self.store.conn().await;
        conn.execute(sea_orm::Statement::from_sql_and_values(
            sea_orm::DbBackend::Postgres,
            &sql,
            vec![msg_id.into()],
        ))
        .await
        .map_err(|e| RszeroError::Queue { message: format!("failed to update message status: {}", e), source: None })?;

        Ok(())
    }

    /// Start a background sweeper that retries pending/failed messages.
    ///
    /// Runs indefinitely until the returned handle is aborted.
    pub fn start_sweeper(self) -> tokio::task::JoinHandle<()> {
        let interval = self.table.retry_interval;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if let Err(e) = self.sweep().await {
                    tracing::error!(error = %e, "message sweeper failed");
                }
            }
        })
    }

    /// Sweep pending messages and attempt to publish them.
    async fn sweep(&self) -> RszeroResult<usize> {
        let sql = format!(
            r#"
            SELECT msg_id, topic, payload, retry_count
            FROM {}
            WHERE status IN ('pending', 'failed')
              AND retry_count < {}
            ORDER BY created_at ASC
            LIMIT 100
            "#,
            self.table.table_name, self.table.max_retries
        );

        let conn = self.store.conn().await;
        let rows = conn
            .query_all(sea_orm::Statement::from_string(sea_orm::DbBackend::Postgres, sql))
            .await
            .map_err(|e| RszeroError::Queue { message: format!("sweep query failed: {}", e), source: None })?;

        let mut processed = 0usize;
        for row in rows {
            let msg_id: String = row.try_get_by_index(0).unwrap_or_default();
            let topic: String = row.try_get_by_index(1).unwrap_or_default();
            let payload: String = row.try_get_by_index(2).unwrap_or_default();
            let retry_count: i32 = row.try_get_by_index(3).unwrap_or(0);

            match self.try_publish(&msg_id, &topic, &payload).await {
                Ok(()) => {
                    processed += 1;
                }
                Err(e) => {
                    let new_retry = retry_count + 1;
                    let new_status = if new_retry >= self.table.max_retries as i32 {
                        "failed"
                    } else {
                        "pending"
                    };
                    let update_sql = format!(
                        "UPDATE {} SET retry_count = $1, status = $2, updated_at = CURRENT_TIMESTAMP WHERE msg_id = $3",
                        self.table.table_name
                    );
                    let _ = conn.execute(sea_orm::Statement::from_sql_and_values(
                        sea_orm::DbBackend::Postgres,
                        &update_sql,
                        vec![new_retry.into(), new_status.into(), msg_id.clone().into()],
                    )).await;
                    tracing::warn!(error = %e, msg_id, retry = new_retry, "sweep publish failed");
                }
            }
        }

        if processed > 0 {
            tracing::info!(processed, "message sweeper processed pending messages");
        }
        Ok(processed)
    }

    /// Get statistics on the message table.
    pub async fn stats(&self) -> RszeroResult<MessageTableStats> {
        let sql = format!(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE status = 'pending') as pending,
                COUNT(*) FILTER (WHERE status = 'published') as published,
                COUNT(*) FILTER (WHERE status = 'failed') as failed
            FROM {}
            "#,
            self.table.table_name
        );

        let conn = self.store.conn().await;
        let row = conn
            .query_one(sea_orm::Statement::from_string(sea_orm::DbBackend::Postgres, sql))
            .await
            .map_err(|e| RszeroError::Queue { message: format!("stats query failed: {}", e), source: None })?;

        let row = row.ok_or_else(|| RszeroError::Queue { message: "stats query returned no rows".into(), source: None })?;
        let pending: i64 = row.try_get_by_index(0).unwrap_or(0);
        let published: i64 = row.try_get_by_index(1).unwrap_or(0);
        let failed: i64 = row.try_get_by_index(2).unwrap_or(0);

        Ok(MessageTableStats {
            pending: pending as u64,
            published: published as u64,
            failed: failed as u64,
        })
    }


}

/// Statistics for the message outbox table.
#[derive(Debug, Clone)]
pub struct MessageTableStats {
    /// Number of pending messages.
    pub pending: u64,
    /// Number of successfully published messages.
    pub published: u64,
    /// Number of failed messages.
    pub failed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_status_display() {
        assert_eq!(MessageStatus::Pending.to_string(), "pending");
        assert_eq!(MessageStatus::Published.to_string(), "published");
        assert_eq!(MessageStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn test_message_table_default() {
        let table = MessageTable::new("outbox");
        assert_eq!(table.table_name, "outbox");
        assert_eq!(table.max_retries, 5);
    }

    #[test]
    fn test_message_table_stats() {
        let stats = MessageTableStats {
            pending: 1,
            published: 10,
            failed: 2,
        };
        assert_eq!(stats.pending, 1);
    }
}
