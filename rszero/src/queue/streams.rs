//! Redis Streams backend for message queue (XADD/XREAD/XGROUP).
//!
//! Provides consumer groups, auto-ack, and pending message recovery
//! on top of Redis 5.0+ Streams.

use crate::error::{RszeroError, RszeroResult};
use crate::queue::{ConsumerFn, Message, QueueBackend};
use fred::prelude::*;
use fred::interfaces::ClientLike;
use fred::types::{CustomCommand, ClusterHash};
use std::time::Duration;

/// Redis Streams queue backend.
pub struct RedisStreamsBackend {
    client: RedisClient,
    group: String,
    consumer: String,
}

impl RedisStreamsBackend {
    /// Create a new Redis Streams backend from URL and optional group name.
    pub async fn new(url: &str, group: Option<&str>) -> RszeroResult<Self> {
        if url.is_empty() {
            return Err(RszeroError::Queue { message: "redis URL is empty".into(), source: None });
        }
        let redis_config = RedisConfig::from_url(url)
            .map_err(|e| RszeroError::Queue { message: format!("redis config failed: {}", e), source: None })?;
        let perf = PerformanceConfig::default();
        let policy = ReconnectPolicy::default();
        let client = RedisClient::new(redis_config, Some(perf), Some(policy));
        client.connect();
        client.wait_for_connect().await
            .map_err(|e| RszeroError::Queue { message: format!("redis connect failed: {}", e), source: None })?;
        tracing::info!("redis streams backend connected");
        Ok(Self {
            client,
            group: group.unwrap_or("rszero-group").to_string(),
            consumer: format!("consumer-{}", uuid::Uuid::new_v4()),
        })
    }

    /// Ensure a consumer group exists for the given topic.
    pub async fn ensure_group(&self, topic: &str) -> RszeroResult<()> {
        let stream_key = format!("rszero:stream:{}", topic);
        let cmd = CustomCommand::new("XGROUP", ClusterHash::FirstKey, false);
        let args: Vec<String> = vec!["CREATE".into(), stream_key, self.group.clone(), "$".into(), "MKSTREAM".into()];
        match self.client.custom::<RedisValue, String>(cmd, args).await {
            Ok(_) => Ok(()),
            Err(e) => {
                if e.to_string().contains("BUSYGROUP") {
                    Ok(())
                } else {
                    Err(RszeroError::Queue { message: format!("xgroup create failed: {}", e), source: None })
                }
            }
        }
    }

    /// Publish a message to a stream topic.
    pub async fn publish(&self, msg: &Message) -> RszeroResult<()> {
        let stream_key = format!("rszero:stream:{}", msg.topic);
        let json = serde_json::to_string(msg)
            .map_err(|e| RszeroError::Queue { message: e.to_string(), source: None })?;
        let cmd = CustomCommand::new("XADD", ClusterHash::FirstKey, false);
        let args: Vec<String> = vec![stream_key, "*".into(), "payload".into(), json];
        self.client.custom::<RedisValue, String>(cmd, args).await
            .map_err(|e| RszeroError::Queue { message: format!("xadd failed: {}", e), source: None })?;
        Ok(())
    }

    /// Read a single message from the consumer group (blocking).
    pub async fn consume(&self, topic: &str) -> RszeroResult<Option<Message>> {
        let stream_key = format!("rszero:stream:{}", topic);
        self.ensure_group(topic).await?;

        let cmd = CustomCommand::new("XREADGROUP", ClusterHash::FirstKey, true);
        let args: Vec<String> = vec![
            "GROUP".into(), self.group.clone(), self.consumer.clone(),
            "COUNT".into(), "1".into(),
            "BLOCK".into(), "5000".into(),
            "STREAMS".into(), stream_key, ">".into(),
        ];

        let result: RedisValue = self.client.custom(cmd, args).await
            .map_err(|e| RszeroError::Queue { message: format!("xreadgroup failed: {}", e), source: None })?;

        let msg = Self::parse_message(result)?;
        if let Some(ref m) = msg {
            // Persist pending ID mapping to Redis Hash for crash recovery
            let hash_key = Self::pending_hash_key(&self.group);
            let _ = self.client.hsetnx::<i64, _, _, _>(&hash_key, &m.id, topic).await;
        }
        Ok(msg)
    }

    /// Acknowledge a message by its stream ID.
    pub async fn ack(&self, topic: &str, msg_id: &str) -> RszeroResult<()> {
        let stream_key = format!("rszero:stream:{}", topic);
        let cmd = CustomCommand::new("XACK", ClusterHash::FirstKey, false);
        let args: Vec<String> = vec![stream_key, self.group.clone(), msg_id.into()];
        self.client.custom::<RedisValue, String>(cmd, args).await
            .map_err(|e| RszeroError::Queue { message: format!("xack failed: {}", e), source: None })?;
        // Remove from persisted pending hash
        let hash_key = Self::pending_hash_key(&self.group);
        let _ = self.client.hdel::<i64, _, _>(&hash_key, msg_id).await;
        Ok(())
    }

    /// Get the pending message count for a topic.
    pub async fn pending(&self, topic: &str) -> RszeroResult<usize> {
        let stream_key = format!("rszero:stream:{}", topic);
        let cmd = CustomCommand::new("XPENDING", ClusterHash::FirstKey, false);
        let args: Vec<String> = vec![stream_key, self.group.clone()];
        let result: RedisValue = self.client.custom(cmd, args).await
            .map_err(|e| RszeroError::Queue { message: format!("xpending failed: {}", e), source: None })?;

        match result {
            RedisValue::Integer(i) => Ok(i as usize),
            RedisValue::Array(arr) if !arr.is_empty() => {
                // XPENDING returns [count, min_id, max_id, [...consumers]]
                if let Some(RedisValue::Integer(count)) = arr.first().cloned() {
                    Ok(count as usize)
                } else {
                    Ok(0)
                }
            }
            _ => Ok(0),
        }
    }

    /// Subscribe to a topic using consumer groups.
    pub async fn subscribe(&self, topic: &str, handler: ConsumerFn) -> RszeroResult<()> {
        let topic = topic.to_string();
        let client = self.client.clone();
        let group = self.group.clone();
        let consumer = self.consumer.clone();

        tokio::spawn(async move {
            let stream_key = format!("rszero:stream:{}", topic);

            // Ensure group exists
            let create_cmd = CustomCommand::new("XGROUP", ClusterHash::FirstKey, false);
            let create_args: Vec<String> = vec!["CREATE".into(), stream_key.clone(), group.clone(), "$".into(), "MKSTREAM".into()];
            let _ = client.custom::<RedisValue, String>(create_cmd, create_args).await;

            loop {
                let read_cmd = CustomCommand::new("XREADGROUP", ClusterHash::FirstKey, true);
                let read_args: Vec<String> = vec![
                    "GROUP".into(), group.clone(), consumer.clone(),
                    "COUNT".into(), "1".into(),
                    "BLOCK".into(), "5000".into(),
                    "STREAMS".into(), stream_key.clone(), ">".into(),
                ];

                match client.custom::<RedisValue, String>(read_cmd, read_args).await {
                    Ok(result) => {
                        if let Ok(Some(msg)) = Self::parse_message(result) {
                            if let Err(e) = handler(msg.clone()).await {
                                tracing::error!(error = %e, "streams consumer handler error");
                            } else {
                                // Auto-ack on success
                                let ack_cmd = CustomCommand::new("XACK", ClusterHash::FirstKey, false);
                                let ack_args: Vec<String> = vec![stream_key.clone(), group.clone(), msg.id.clone()];
                                let _ = client.custom::<RedisValue, String>(ack_cmd, ack_args).await;
                                // Remove from persisted pending hash
                                let hash_key = format!("rszero:pending:{}", group);
                                let _ = client.hdel::<i64, _, _>(&hash_key, &msg.id).await;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "streams subscribe error");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(())
    }

    /// Claim pending messages that have been idle for longer than `min_idle_ms`.
    pub async fn claim_pending(&self, topic: &str, min_idle_ms: u64, count: usize) -> RszeroResult<Vec<Message>> {
        let stream_key = format!("rszero:stream:{}", topic);
        let cmd = CustomCommand::new("XAUTOCLAIM", ClusterHash::FirstKey, false);
        let args: Vec<String> = vec![
            stream_key, self.group.clone(), self.consumer.clone(),
            min_idle_ms.to_string(), "0".into(),
            "COUNT".into(), count.to_string(),
        ];

        let result: RedisValue = self.client.custom(cmd, args).await
            .map_err(|e| RszeroError::Queue { message: format!("xautoclaim failed: {}", e), source: None })?;

        let mut messages = Vec::new();
        if let RedisValue::Array(outer) = result {
            // XAUTOCLAIM returns [next_start_id, [msg1, msg2, ...]]
            if let Some(RedisValue::Array(items)) = outer.get(1) {
                for item in items {
                    if let Ok(Some(msg)) = Self::parse_stream_entry(item.clone()) {
                        messages.push(msg);
                    }
                }
            }
        }
        Ok(messages)
    }

    /// Close the Redis connection.
    pub async fn close(&self) -> RszeroResult<()> {
        self.client.quit().await
            .map_err(|e| RszeroError::Queue { message: format!("redis quit failed: {}", e), source: None })?;
        Ok(())
    }

    /// Get the topic for a pending message ID from Redis.
    pub async fn topic_for_id(&self, msg_id: &str) -> Option<String> {
        let hash_key = Self::pending_hash_key(&self.group);
        self.client.hget::<Option<String>, _, _>(&hash_key, msg_id).await.ok().flatten()
    }

    fn pending_hash_key(group: &str) -> String {
        format!("rszero:pending:{}", group)
    }

    // ─── Internal helpers ───────────────────────────────────────────────────

    fn parse_message(result: RedisValue) -> RszeroResult<Option<Message>> {
        match result {
            RedisValue::Null => Ok(None),
            RedisValue::Array(outer) if !outer.is_empty() => {
                // [[stream_key, [[id, [payload_key, payload_value]], ...]]]
                if let Some(RedisValue::Array(stream_arr)) = outer.first() {
                    if let Some(RedisValue::Array(entries)) = stream_arr.get(1) {
                        if let Some(RedisValue::Array(entry)) = entries.first() {
                            return Self::parse_stream_entry(RedisValue::Array(entry.clone()));
                        }
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn parse_stream_entry(entry: RedisValue) -> RszeroResult<Option<Message>> {
        match entry {
            RedisValue::Array(arr) if arr.len() >= 2 => {
                let id = match &arr[0] {
                    RedisValue::String(s) => s.to_string(),
                    _ => return Ok(None),
                };
                let fields = match &arr[1] {
                    RedisValue::Array(f) => f,
                    _ => return Ok(None),
                };

                // Look for "payload" field
                for chunk in fields.chunks(2) {
                    if chunk.len() == 2 {
                        let key = match &chunk[0] {
                            RedisValue::String(s) => s.to_string(),
                            _ => continue,
                        };
                        if key == "payload" {
                            let value = match &chunk[1] {
                                RedisValue::String(s) => s.to_string(),
                                _ => continue,
                            };
                            let mut msg: Message = serde_json::from_str(&value)
                                .map_err(|e| RszeroError::Queue { message: format!("deserialize failed: {}", e), source: None })?;
                            msg.id = id;
                            return Ok(Some(msg));
                        }
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}

#[async_trait::async_trait]
impl QueueBackend for RedisStreamsBackend {
    async fn publish(&self, msg: Message) -> RszeroResult<()> {
        RedisStreamsBackend::publish(self, &msg).await
    }

    async fn consume(&self, topic: &str) -> RszeroResult<Option<Message>> {
        RedisStreamsBackend::consume(self, topic).await
    }

    async fn pending(&self, topic: &str) -> RszeroResult<usize> {
        RedisStreamsBackend::pending(self, topic).await
    }

    async fn ack(&self, msg_id: &str) -> RszeroResult<()> {
        if let Some(topic) = self.topic_for_id(msg_id).await {
            RedisStreamsBackend::ack(self, &topic, msg_id).await
        } else {
            tracing::warn!(msg_id, "cannot ack: unknown topic");
            Ok(())
        }
    }

    async fn nack(&self, msg_id: &str, _requeue: bool) -> RszeroResult<()> {
        tracing::debug!(msg_id, "redis streams nack (no-op, message remains pending)");
        Ok(())
    }

    async fn subscribe(&self, topic: &str, handler: ConsumerFn) -> RszeroResult<()> {
        RedisStreamsBackend::subscribe(self, topic, handler).await
    }

    async fn close(&self) -> RszeroResult<()> {
        RedisStreamsBackend::close(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_null() {
        let result = RedisStreamsBackend::parse_message(RedisValue::Null);
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_parse_empty_array() {
        let result = RedisStreamsBackend::parse_message(RedisValue::Array(vec![]));
        assert!(result.unwrap().is_none());
    }
}
