//! WebSocket support for rszero REST API.
//!
//! Provides WebSocket upgrade handling, broadcast channels, room management,
//! connection management, and production-grade heartbeat / reconnection.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use axum::response::IntoResponse;

/// WebSocket heartbeat configuration.
#[derive(Clone, Debug)]
pub struct HeartbeatConfig {
    /// Interval between heartbeat pings.
    pub interval: Duration,
    /// Maximum time to wait for a pong before disconnecting.
    pub timeout: Duration,
    /// Whether to enable heartbeat.
    pub enabled: bool,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(60),
            enabled: true,
        }
    }
}

/// WebSocket message ACK configuration.
#[derive(Clone, Debug)]
pub struct AckConfig {
    /// Whether to enable message ACK.
    pub enabled: bool,
    /// Timeout for client to ACK a message.
    pub timeout: Duration,
    /// Maximum retransmission attempts.
    pub max_retries: u32,
}

impl Default for AckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout: Duration::from_secs(5),
            max_retries: 3,
        }
    }
}

/// An unacknowledged message waiting for client confirmation.
#[derive(Debug, Clone)]
struct UnackedMessage {
    payload: String,
    retry_count: u32,
    sent_at: std::time::Instant,
}

/// WebSocket connection state.
#[derive(Debug)]
struct WsConnection {
    id: String,
    last_pong: std::sync::atomic::AtomicU64,
    room: Option<String>,
    unacked: HashMap<String, UnackedMessage>,
}

impl Clone for WsConnection {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            last_pong: std::sync::atomic::AtomicU64::new(
                self.last_pong.load(std::sync::atomic::Ordering::Relaxed)
            ),
            room: self.room.clone(),
            unacked: self.unacked.clone(),
        }
    }
}

/// WebSocket connection manager with heartbeat and room support.
pub struct WsManager {
    tx: broadcast::Sender<String>,
    connections: Arc<RwLock<Vec<WsConnection>>>,
    rooms: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
    heartbeat: HeartbeatConfig,
    ack: AckConfig,
}

impl Clone for WsManager {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            connections: self.connections.clone(),
            rooms: self.rooms.clone(),
            heartbeat: self.heartbeat.clone(),
            ack: self.ack.clone(),
        }
    }
}

impl WsManager {
    /// Create a new WebSocket manager.
    pub fn new() -> Self {
        Self::with_configs(HeartbeatConfig::default(), AckConfig::default())
    }

    /// Create a manager with custom heartbeat settings.
    pub fn with_heartbeat(heartbeat: HeartbeatConfig) -> Self {
        Self::with_configs(heartbeat, AckConfig::default())
    }

    /// Create a manager with custom heartbeat and ACK settings.
    pub fn with_configs(heartbeat: HeartbeatConfig, ack: AckConfig) -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            tx,
            connections: Arc::new(RwLock::new(Vec::new())),
            rooms: Arc::new(RwLock::new(HashMap::new())),
            heartbeat,
            ack,
        }
    }

    /// Broadcast a message to all connected clients.
    pub async fn broadcast(&self, message: &str) {
        let _ = self.tx.send(message.to_string());
    }

    /// Broadcast a message to a specific room.
    pub async fn broadcast_room(&self, room: &str, message: &str) {
        let rooms = self.rooms.read().await;
        if let Some(tx) = rooms.get(room) {
            let _ = tx.send(message.to_string());
        }
    }

    /// Get the number of active connections.
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Get the number of connections in a room.
    pub async fn room_count(&self, room: &str) -> usize {
        let conns = self.connections.read().await;
        conns.iter().filter(|c| c.room.as_deref() == Some(room)).count()
    }

    /// Get list of active room names.
    pub async fn rooms(&self) -> Vec<String> {
        let rooms = self.rooms.read().await;
        rooms.keys().cloned().collect()
    }

    /// Handle a WebSocket upgrade request.
    pub fn handle_upgrade(&self, ws: WebSocketUpgrade) -> impl IntoResponse {
        let manager = self.clone();
        ws.on_upgrade(move |socket| Self::handle_connection(socket, manager))
    }

    async fn handle_connection(mut socket: WebSocket, manager: WsManager) {
        let client_id = format!("client-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let conn = WsConnection {
            id: client_id.clone(),
            last_pong: std::sync::atomic::AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            ),
            room: None,
            unacked: HashMap::new(),
        };

        manager.connections.write().await.push(conn);
        tracing::info!(client = %client_id, "websocket connected");

        let mut rx = manager.tx.subscribe();
        let mut ping_interval = interval(manager.heartbeat.interval);
        let mut ack_interval = interval(manager.ack.timeout);
        let heartbeat_enabled = manager.heartbeat.enabled;
        let ack_enabled = manager.ack.enabled;
        let timeout_secs = manager.heartbeat.timeout.as_secs();
        let ack_timeout = manager.ack.timeout;
        let ack_max_retries = manager.ack.max_retries;

        loop {
            tokio::select! {
                // Send periodic heartbeat ping
                _ = ping_interval.tick(), if heartbeat_enabled => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let conns = manager.connections.read().await;
                    if let Some(c) = conns.iter().find(|c| c.id == client_id) {
                        let last = c.last_pong.load(std::sync::atomic::Ordering::Relaxed);
                        if now.saturating_sub(last) > timeout_secs {
                            tracing::warn!(client = %client_id, "websocket heartbeat timeout");
                            break;
                        }
                    }
                    drop(conns);
                    if socket.send(Message::Ping(vec![])).await.is_err() {
                        break;
                    }
                }
                // Retransmit unacknowledged messages
                _ = ack_interval.tick(), if ack_enabled => {
                    let mut conns = manager.connections.write().await;
                    if let Some(c) = conns.iter_mut().find(|c| c.id == client_id) {
                        let now = std::time::Instant::now();
                        let mut to_remove = Vec::new();
                        for (msg_id, unacked) in c.unacked.iter_mut() {
                            if now.duration_since(unacked.sent_at) > ack_timeout {
                                if unacked.retry_count >= ack_max_retries {
                                    tracing::warn!(client = %client_id, msg_id, "message ack exhausted, dropping");
                                    to_remove.push(msg_id.clone());
                                    continue;
                                }
                                unacked.retry_count += 1;
                                unacked.sent_at = now;
                                let payload = format!("{{\"id\":\"{}\",\"payload\":{}}}", msg_id, unacked.payload);
                                tracing::debug!(client = %client_id, msg_id, retry = unacked.retry_count, "retransmitting message");
                                let _ = socket.send(Message::Text(payload)).await;
                            }
                        }
                        for msg_id in to_remove {
                            c.unacked.remove(&msg_id);
                        }
                    }
                    drop(conns);
                }
                // Broadcast message to client
                Ok(msg) = rx.recv() => {
                    if ack_enabled {
                        let msg_id = format!("msg-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                        let payload = format!("{{\"id\":\"{}\",\"payload\":{}}}", msg_id, msg);
                        {
                            let mut conns = manager.connections.write().await;
                            if let Some(c) = conns.iter_mut().find(|c| c.id == client_id) {
                                c.unacked.insert(msg_id.clone(), UnackedMessage {
                                    payload: msg,
                                    retry_count: 0,
                                    sent_at: std::time::Instant::now(),
                                });
                            }
                            drop(conns);
                        }
                        if socket.send(Message::Text(payload)).await.is_err() {
                            break;
                        }
                    } else {
                        if socket.send(Message::Text(msg)).await.is_err() {
                            break;
                        }
                    }
                }
                // Receive message from client
                result = socket.recv() => {
                    match result {
                        Some(Ok(Message::Text(text))) => {
                            if ack_enabled {
                                // Check for ACK: {"ack": "msg-id"}
                                if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                                    if let Some(acked_id) = cmd.get("ack").and_then(|v| v.as_str()) {
                                        let mut conns = manager.connections.write().await;
                                        if let Some(c) = conns.iter_mut().find(|c| c.id == client_id) {
                                            c.unacked.remove(acked_id);
                                            tracing::debug!(client = %client_id, msg_id = %acked_id, "message acked");
                                        }
                                        continue;
                                    }
                                }
                            }
                            Self::handle_client_message(&manager, &client_id, &text).await;
                        }
                        Some(Ok(Message::Pong(_))) => {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let mut conns = manager.connections.write().await;
                            if let Some(c) = conns.iter_mut().find(|c| c.id == client_id) {
                                c.last_pong.store(now, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        _ => {}
                    }
                }
            }
        }

        {
            let mut conns = manager.connections.write().await;
            let removed_room = conns.iter().find(|c| c.id == client_id).and_then(|c| c.room.clone());
            conns.retain(|c| c.id != client_id);
            drop(conns);

            // Clean up empty rooms
            if let Some(room) = removed_room {
                let room_empty = manager.room_count(&room).await == 0;
                if room_empty {
                    let mut rooms = manager.rooms.write().await;
                    rooms.remove(&room);
                    tracing::debug!(room, "removed empty room");
                }
            }
        }
        tracing::info!(client = %client_id, "websocket disconnected");
    }

    async fn handle_client_message(manager: &WsManager, client_id: &str, text: &str) {
        tracing::debug!(client = %client_id, msg = %text, "received message");

        // Simple room protocol: {"join": "room-name"} or {"leave": "room-name"}
        if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(text) {
            if let Some(room) = cmd.get("join").and_then(|v| v.as_str()) {
                let mut rooms = manager.rooms.write().await;
                rooms.entry(room.to_string()).or_insert_with(|| {
                    let (tx, _) = broadcast::channel(256);
                    tx
                });
                drop(rooms);
                let mut conns = manager.connections.write().await;
                if let Some(c) = conns.iter_mut().find(|c| c.id == client_id) {
                    c.room = Some(room.to_string());
                }
                tracing::info!(client = %client_id, room, "joined room");
            } else if let Some(room) = cmd.get("leave").and_then(|v| v.as_str()) {
                let mut conns = manager.connections.write().await;
                if let Some(c) = conns.iter_mut().find(|c| c.id == client_id) {
                    if c.room.as_deref() == Some(room) {
                        c.room = None;
                    }
                }
                tracing::info!(client = %client_id, room, "left room");
            }
        }
    }

    /// Get the number of unacknowledged messages for a client.
    pub async fn unacked_count(&self, client_id: &str) -> usize {
        let conns = self.connections.read().await;
        conns.iter()
            .find(|c| c.id == client_id)
            .map(|c| c.unacked.len())
            .unwrap_or(0)
    }

    /// Get total unacknowledged messages across all connections.
    pub async fn total_unacked_count(&self) -> usize {
        let conns = self.connections.read().await;
        conns.iter().map(|c| c.unacked.len()).sum()
    }
}

impl Default for WsManager {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ws_manager_broadcast() {
        let manager = WsManager::new();
        manager.broadcast("hello").await;
        assert_eq!(manager.connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_ws_manager_room() {
        let manager = WsManager::new();
        manager.broadcast_room("test-room", "hello").await;
        assert_eq!(manager.room_count("test-room").await, 0);
        assert!(manager.rooms().await.is_empty());
    }

    #[test]
    fn test_ws_manager_default() {
        let _manager = WsManager::default();
    }

    #[test]
    fn test_heartbeat_config_default() {
        let cfg = HeartbeatConfig::default();
        assert_eq!(cfg.interval, Duration::from_secs(30));
        assert_eq!(cfg.timeout, Duration::from_secs(60));
        assert!(cfg.enabled);
    }

    #[test]
    fn test_ack_config_default() {
        let cfg = AckConfig::default();
        assert_eq!(cfg.timeout, Duration::from_secs(5));
        assert_eq!(cfg.max_retries, 3);
        assert!(cfg.enabled);
    }
}
