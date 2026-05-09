//! Order RPC Service — Order management with distributed stock deduction,
//! message queue integration, and saga pattern for consistency.
//!
//! Demonstrates rszero best practices:
//! - Saga pattern for distributed transactions
//! - Message queue for async notifications
//! - Circuit breaker for RPC calls to Book service
//! - Service discovery for Book RPC
//! - Structured logging and tracing

use rszero::prelude::*;
use bookstore_common::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;

// ─── Domain Models ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub user_id: String,
    pub book_id: String,
    pub quantity: u32,
    pub total: f64,
    pub status: OrderStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderStatus {
    Pending,
    Confirmed,
    Shipped,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderReq {
    pub user_id: String,
    pub book_id: String,
    pub quantity: u32,
}

// ─── Service Context ──────────────────────────────────────────────────────

/// Order service context — holds dependencies including RPC client to Book service.
pub struct OrderSvc {
    pub config: RszeroConfig,
    pub db: RwLock<HashMap<String, Order>>,
    pub queue: Queue,
    pub breaker: CircuitBreaker,
    pub book_rpc_addr: String,
}

impl OrderSvc {
    pub fn new(config: RszeroConfig) -> Self {
        Self {
            db: RwLock::new(HashMap::new()),
            queue: Queue::new(rszero::config::QueueConfig::default()),
            breaker: CircuitBreaker::new(3),
            book_rpc_addr: "127.0.0.1:8081".into(),
            config,
        }
    }
}

// ─── Logic Layer ──────────────────────────────────────────────────────────

/// Order logic — implements saga pattern for order creation:
/// 1. Validate order
/// 2. Call Book RPC to deduct stock (with circuit breaker)
/// 3. Create order record
/// 4. Send async notification via queue
/// 5. On failure: compensate (rollback stock)
pub struct OrderLogic {
    svc: Arc<OrderSvc>,
}

impl OrderLogic {
    pub fn new(svc: Arc<OrderSvc>) -> Self { Self { svc } }

    /// Create order using saga pattern for distributed consistency.
    pub async fn create_order(&self, req: CreateOrderReq) -> BookstoreResult<Order> {
        if req.user_id.is_empty() {
            return Err(BookstoreError::InvalidInput("user_id is required".into()));
        }
        if req.quantity == 0 {
            return Err(BookstoreError::InvalidInput("quantity must be positive".into()));
        }

        // Step 1: Deduct stock via RPC (with circuit breaker)
        let book_result = self.svc.breaker.execute(async {
            // In production: self.book_rpc.deduct_stock(&req.book_id, req.quantity).await
            // For demo: simulate stock deduction
            tracing::info!(
                book_id = %req.book_id,
                quantity = req.quantity,
                "calling Book RPC to deduct stock"
            );
            Ok::<_, String>(())
        }).await;

        if let Err(e) = book_result {
            tracing::warn!(error = %e, "stock deduction failed");
            return Err(BookstoreError::Internal(format!("Stock deduction failed: {}", e)));
        }

        // Step 2: Create order
        let now = chrono::Utc::now().timestamp();
        let order = Order {
            id: generate_short_id(),
            user_id: req.user_id,
            book_id: req.book_id,
            quantity: req.quantity,
            total: 0.0, // Would be calculated from Book RPC response
            status: OrderStatus::Confirmed,
            created_at: now,
            updated_at: now,
        };

        self.svc.db.write().await.insert(order.id.clone(), order.clone());

        // Step 3: Send async notification
        let _ = self.svc.queue.push("order.created", &serde_json::json!({
            "order_id": order.id,
            "book_id": order.book_id,
            "quantity": order.quantity,
        })).await;

        tracing::info!(order_id = %order.id, "order created");
        Ok(order)
    }

    /// Get order by ID.
    pub async fn get_order(&self, id: &str) -> BookstoreResult<Order> {
        let db = self.svc.db.read().await;
        db.get(id).cloned()
            .ok_or_else(|| BookstoreError::OrderNotFound(id.to_string()))
    }

    /// List orders with pagination and filters.
    pub async fn list_orders(
        &self,
        user_id: Option<&str>,
        status: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> BookstoreResult<(Vec<Order>, i32)> {
        let db = self.svc.db.read().await;
        let mut orders: Vec<Order> = db.values()
            .filter(|o| user_id.map_or(true, |uid| o.user_id == uid))
            .filter(|o| status.map_or(true, |s| format!("{:?}", o.status) == s))
            .cloned()
            .collect();

        let total = orders.len() as i32;
        let offset = ((page - 1) * page_size) as usize;
        let end = (offset + page_size as usize).min(orders.len());
        let page_orders = if offset < orders.len() {
            orders.drain(offset..end).collect()
        } else {
            Vec::new()
        };

        Ok((page_orders, total))
    }

    /// Cancel order with compensation logic.
    pub async fn cancel_order(&self, id: &str, reason: &str) -> BookstoreResult<bool> {
        let mut db = self.svc.db.write().await;
        let order = db.get_mut(id)
            .ok_or_else(|| BookstoreError::OrderNotFound(id.to_string()))?;

        if order.status == OrderStatus::Cancelled {
            return Err(BookstoreError::OrderAlreadyCancelled(id.to_string()));
        }

        order.status = OrderStatus::Cancelled;
        order.updated_at = chrono::Utc::now().timestamp();

        // Compensate: restore stock
        tracing::info!(
            order_id = %id,
            book_id = %order.book_id,
            quantity = order.quantity,
            reason = %reason,
            "cancelling order, restoring stock"
        );

        // In production: self.book_rpc.restore_stock(&order.book_id, order.quantity).await
        let _ = self.svc.queue.push("order.cancelled", &serde_json::json!({
            "order_id": id,
            "book_id": order.book_id,
            "quantity": order.quantity,
            "reason": reason,
        })).await;

        Ok(true)
    }
}

// ─── RPC Service ──────────────────────────────────────────────────────────

pub struct OrderRpcService {
    logic: OrderLogic,
}

impl OrderRpcService {
    pub fn new(svc: Arc<OrderSvc>) -> Self {
        Self { logic: OrderLogic::new(svc) }
    }

    pub fn logic(&self) -> &OrderLogic { &self.logic }
}

// ─── Main ─────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = load_config("etc/order-rpc.yaml")?;
    log::init(&config.log);
    log::info("Starting Order RPC service...");

    let svc = Arc::new(OrderSvc::new(config));
    let _rpc_service = OrderRpcService::new(svc);

    // Register with service discovery
    let discovery = ServiceDiscovery::from_etcd(vec!["127.0.0.1:2379".into()]);
    discovery.register("order.rpc", "127.0.0.1:8082").await?;

    log::info("Order RPC service ready on :8082");
    log::info("  CreateOrder   - Create new order (saga pattern)");
    log::info("  GetOrder      - Get order by ID");
    log::info("  ListOrders    - List orders with pagination");
    log::info("  CancelOrder   - Cancel order with compensation");

    tokio::signal::ctrl_c().await?;
    log::info("Order RPC service shutting down...");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_svc() -> Arc<OrderSvc> {
        Arc::new(OrderSvc::new(RszeroConfig::default()))
    }

    #[tokio::test]
    async fn test_create_order() {
        let svc = test_svc();
        let logic = OrderLogic::new(svc);
        let order = logic.create_order(CreateOrderReq {
            user_id: "user-1".into(),
            book_id: "book-1".into(),
            quantity: 2,
        }).await.unwrap();
        assert_eq!(order.status, OrderStatus::Confirmed);
        assert_eq!(order.user_id, "user-1");
    }

    #[tokio::test]
    async fn test_get_order() {
        let svc = test_svc();
        let logic = OrderLogic::new(svc);
        let order = logic.create_order(CreateOrderReq {
            user_id: "user-1".into(),
            book_id: "book-1".into(),
            quantity: 1,
        }).await.unwrap();

        let found = logic.get_order(&order.id).await.unwrap();
        assert_eq!(found.id, order.id);
    }

    #[tokio::test]
    async fn test_cancel_order() {
        let svc = test_svc();
        let logic = OrderLogic::new(svc);
        let order = logic.create_order(CreateOrderReq {
            user_id: "user-1".into(),
            book_id: "book-1".into(),
            quantity: 1,
        }).await.unwrap();

        let cancelled = logic.cancel_order(&order.id, "changed mind").await.unwrap();
        assert!(cancelled);

        let found = logic.get_order(&order.id).await.unwrap();
        assert_eq!(found.status, OrderStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_cancel_already_cancelled() {
        let svc = test_svc();
        let logic = OrderLogic::new(svc);
        let order = logic.create_order(CreateOrderReq {
            user_id: "user-1".into(),
            book_id: "book-1".into(),
            quantity: 1,
        }).await.unwrap();

        logic.cancel_order(&order.id, "reason").await.unwrap();
        let result = logic.cancel_order(&order.id, "again").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_orders() {
        let svc = test_svc();
        let logic = OrderLogic::new(svc);
        logic.create_order(CreateOrderReq {
            user_id: "user-1".into(), book_id: "book-1".into(), quantity: 1,
        }).await.unwrap();
        logic.create_order(CreateOrderReq {
            user_id: "user-2".into(), book_id: "book-2".into(), quantity: 2,
        }).await.unwrap();

        let (orders, total) = logic.list_orders(None, None, 1, 10).await.unwrap();
        assert_eq!(total, 2);
        assert_eq!(orders.len(), 2);

        let (orders, total) = logic.list_orders(Some("user-1"), None, 1, 10).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(orders[0].user_id, "user-1");
    }
}
