//! Configuration hot-reloading support.
//!
//! Watches config files for changes using OS-level file system events (via `notify`),
//! and triggers callbacks on updates. Supports multiple watched files and custom
//! reload callbacks.

use crate::config::RszeroConfig;
use crate::error::RszeroResult;
use std::collections::HashMap;
use notify::Watcher;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, RwLock};
use tokio::time::Duration;

/// Configuration change callback type.
pub type ConfigCallback = Box<dyn Fn(&RszeroConfig) + Send + Sync>;

/// Configuration watcher that monitors files for changes using OS-level events.
pub struct ConfigWatcher {
    receiver: watch::Receiver<RszeroConfig>,
    callbacks: Arc<RwLock<Vec<ConfigCallback>>>,
    _handle: tokio::task::JoinHandle<()>,
}

impl ConfigWatcher {
    /// Start watching a single config file for changes.
    ///
    /// Uses `notify` for OS-level file system events (inotify/epoll/kqueue).
    pub fn start(path: &str) -> RszeroResult<Self> {
        let path = Path::new(path).canonicalize().unwrap_or_else(|_| Path::new(path).to_path_buf());
        let initial = crate::config::load_config(path.to_str().unwrap_or(""))?;
        let (sender, receiver) = watch::channel(initial);
        let callbacks: Arc<RwLock<Vec<ConfigCallback>>> = Arc::new(RwLock::new(Vec::new()));
        let callbacks_clone = callbacks.clone();

        let handle = tokio::spawn(async move {
            let (tx, mut rx) = mpsc::channel::<()>(1);
            let mut watcher = match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    if event.kind.is_modify() || event.kind.is_create() {
                        let _ = tx.try_send(());
                    }
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!(error = %e, "failed to create file watcher");
                    return;
                }
            };

            if let Err(e) = watcher.watch(&path, notify::RecursiveMode::NonRecursive) {
                tracing::error!(error = %e, "failed to watch config file");
                return;
            }

            // Debounce: wait 300ms after last event before reloading
            let mut pending = false;
            loop {
                tokio::select! {
                    _ = rx.recv() => {
                        pending = true;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(300)), if pending => {
                        pending = false;
                        if let Ok(new_config) = crate::config::load_config(path.to_str().unwrap_or("")) {
                            let _ = sender.send(new_config.clone());
                            tracing::info!(path = %path.display(), "config reloaded");
                            let cbs = callbacks_clone.read().await;
                            for cb in cbs.iter() {
                                cb(&new_config);
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            receiver,
            callbacks,
            _handle: handle,
        })
    }

    /// Get the current configuration.
    pub fn get(&self) -> RszeroConfig {
        self.receiver.borrow().clone()
    }

    /// Subscribe to configuration changes.
    pub fn subscribe(&self) -> watch::Receiver<RszeroConfig> {
        self.receiver.clone()
    }

    /// Register a callback invoked on every config reload.
    pub async fn on_change<F>(&self, callback: F)
    where
        F: Fn(&RszeroConfig) + Send + Sync + 'static,
    {
        let mut cbs = self.callbacks.write().await;
        cbs.push(Box::new(callback));
    }
}

/// Multi-file configuration watcher.
pub struct MultiConfigWatcher {
    watchers: HashMap<String, ConfigWatcher>,
}

impl MultiConfigWatcher {
    /// Create an empty multi-file watcher.
    pub fn new() -> Self {
        Self {
            watchers: HashMap::new(),
        }
    }

    /// Watch a config file and return a receiver for its changes.
    pub fn watch(&mut self, path: &str) -> RszeroResult<watch::Receiver<RszeroConfig>> {
        let watcher = ConfigWatcher::start(path)?;
        let rx = watcher.subscribe();
        self.watchers.insert(path.to_string(), watcher);
        Ok(rx)
    }

    /// Get a watched config by path.
    pub fn get(&self, path: &str) -> Option<RszeroConfig> {
        self.watchers.get(path).map(|w| w.get())
    }
}

impl Default for MultiConfigWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_watcher_get() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("rszero_watcher_test.yaml");
        std::fs::write(&path, "name: test\nhost: 127.0.0.1\nport: 8080\n").unwrap();

        let watcher = ConfigWatcher::start(path.to_str().unwrap()).unwrap();
        let config = watcher.get();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn test_config_watcher_on_change() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("rszero_watcher_cb.yaml");
        std::fs::write(&path, "name: test\nhost: 127.0.0.1\nport: 8080\n").unwrap();

        let watcher = ConfigWatcher::start(path.to_str().unwrap()).unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        watcher.on_change(move |cfg| {
            let _ = tx.try_send(cfg.port);
        }).await;

        // Wait for watcher to start
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Modify the file
        std::fs::write(&path, "name: test\nhost: 127.0.0.1\nport: 9090\n").unwrap();

        // Wait for debounce + callback (kqueue can be slow on macOS)
        let port = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await;
        assert!(port.is_ok(), "timed out waiting for config reload");
        assert_eq!(port.unwrap().unwrap(), 9090);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_multi_config_watcher() {
        let watcher = MultiConfigWatcher::new();
        assert!(watcher.watchers.is_empty());
    }
}
