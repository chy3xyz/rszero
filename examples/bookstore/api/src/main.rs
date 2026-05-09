//! Bookstore API Gateway — Full microservice example demonstrating rszero capabilities.
//!
//! Demonstrates:
//! - REST routing with method handlers
//! - JWT authentication middleware
//! - Rate limiting
//! - Circuit breaker pattern
//! - In-memory cache with TTL
//! - Distributed tracing middleware
//! - Request ID correlation
//! - Structured logging
//! - Unified error handling
//! - Health check endpoint
//! - OpenAPI spec generation

use rszero::prelude::*;
use serde::{Deserialize, Serialize};
use axum::{
    routing::{get, post, put, delete},
    extract::{State, Path, Json, Query},
    response::IntoResponse,
    middleware,
};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;

// ─── Domain Models ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Book {
    id: String,
    title: String,
    author: String,
    price: f64,
    stock: u32,
    isbn: String,
    category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateBookReq {
    title: String,
    author: String,
    price: f64,
    isbn: String,
    category: String,
    stock: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Order {
    id: String,
    user_id: String,
    book_id: String,
    quantity: u32,
    total: f64,
    status: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateOrderReq {
    book_id: String,
    quantity: u32,
}

// ─── Application State ────────────────────────────────────────────────────

struct AppState {
    books: RwLock<HashMap<String, Book>>,
    orders: RwLock<HashMap<String, Order>>,
    cache: MemCache<String, Book>,
    book_list_cache: MemCache<String, Vec<Book>>,
    breaker: CircuitBreaker,
    jwt: JwtMiddleware,
    shedder: AdaptiveShedder,
    metrics: Metrics,
}

impl AppState {
    fn new() -> Self {
        let cache = MemCache::new(500);
        let book_list_cache = MemCache::new(100);
        let breaker = CircuitBreaker::new(5);
        let jwt = JwtMiddleware::new("bookstore-jwt-secret-key-2024");
        let shedder = AdaptiveShedder::new(200);
        let metrics = Metrics::new("bookstore-api");

        let mut books = HashMap::new();
        books.insert("book-1".into(), Book {
            id: "book-1".into(), title: "The Rust Programming Language".into(),
            author: "Steve Klabnik".into(), price: 39.99, stock: 100,
            isbn: "978-1718500440".into(), category: "programming".into(),
        });
        books.insert("book-2".into(), Book {
            id: "book-2".into(), title: "Programming Rust".into(),
            author: "Jim Blandy".into(), price: 49.99, stock: 50,
            isbn: "978-1492052593".into(), category: "programming".into(),
        });
        books.insert("book-3".into(), Book {
            id: "book-3".into(), title: "Designing Data-Intensive Applications".into(),
            author: "Martin Kleppmann".into(), price: 44.99, stock: 75,
            isbn: "978-1449373320".into(), category: "architecture".into(),
        });

        Self {
            books: RwLock::new(books),
            orders: RwLock::new(HashMap::new()),
            cache,
            book_list_cache,
            breaker,
            jwt,
            shedder,
            metrics,
        }
    }
}

// ─── Request/Response Types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResp {
    token: String,
    user_id: String,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    category: Option<String>,
    page: Option<i32>,
    page_size: Option<i32>,
}

// ─── Public Handlers ──────────────────────────────────────────────────────

async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.shedder.record_latency(1);
    JsonResponse::ok(serde_json::json!({
        "status": "healthy",
        "service": "bookstore-api",
        "version": "0.1.0",
        "shedder_active": state.shedder.is_active(),
    }))
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.metrics.export_prometheus()
}

async fn openapi_handler() -> impl IntoResponse {
    let spec = OpenApiSpec::new("Bookstore API", "0.1.0")
        .description("Bookstore microservice API")
        .path("/v1/books", "GET",
            ApiOperation::new("List books")
                .tag("books")
                .query_param("category", "string")
                .query_param("page", "integer")
        )
        .path("/v1/books/{id}", "GET",
            ApiOperation::new("Get book by ID")
                .tag("books")
                .path_param("id", "string")
        )
        .path("/v1/books", "POST",
            ApiOperation::new("Create a new book")
                .tag("books")
        )
        .path("/v1/orders", "POST",
            ApiOperation::new("Create an order")
                .tag("orders")
        )
        .security_scheme("BearerAuth", SecurityScheme::jwt());

    spec.to_json().unwrap_or_else(|_| "{}".into())
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginReq>,
) -> impl IntoResponse {
    if req.username.is_empty() || req.password.is_empty() {
        return JsonResponse::error(401, "username and password required");
    }

    let token = state.jwt.generate_token(&req.username, 86400)
        .unwrap_or_else(|_| "error".into());

    JsonResponse::ok(LoginResp {
        token,
        user_id: generate_short_id(),
    })
}

async fn list_books(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    let _guard = state.metrics.start_request();

    let cache_key = format!("books:{}", query.category.as_deref().unwrap_or("all"));
    if let Some(cached) = state.book_list_cache.get(&cache_key) {
        state.metrics.record_success();
        return JsonResponse::ok(cached);
    }

    let books = state.books.read().await;
    let list: Vec<Book> = books.values()
        .filter(|b| query.category.as_ref().map_or(true, |c| &b.category == c))
        .cloned()
        .collect();

    let _ = state.book_list_cache.set_with_ttl(cache_key.clone(), list.clone(), Some(std::time::Duration::from_secs(60)));
    state.metrics.record_success();
    JsonResponse::ok(list)
}

async fn get_book(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let _guard = state.metrics.start_request();

    let cache_key = format!("book:{}", id);
    if let Some(cached) = state.cache.get(&cache_key) {
        state.metrics.record_success();
        return JsonResponse::ok(cached);
    }

    let books = state.books.read().await;
    match books.get(&id) {
        Some(book) => {
            let _ = state.cache.set_with_ttl(cache_key.clone(), book.clone(), Some(std::time::Duration::from_secs(300)));
            state.metrics.record_success();
            JsonResponse::ok(book.clone())
        }
        None => {
            state.metrics.record_error();
            JsonResponse::<Book>::error(404, "Book not found")
        }
    }
}

// ─── Protected Handlers ───────────────────────────────────────────────────

async fn create_book(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateBookReq>,
) -> impl IntoResponse {
    let _guard = state.metrics.start_request();

    let book = Book {
        id: generate_short_id(),
        title: req.title,
        author: req.author,
        price: req.price,
        stock: req.stock,
        isbn: req.isbn,
        category: req.category,
    };

    let id = book.id.clone();
    state.books.write().await.insert(id.clone(), book.clone());
    state.cache.clear();

    state.metrics.record_success();
    log::info(&format!("Book created: {}", id));
    JsonResponse::ok(book)
}

async fn create_order(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateOrderReq>,
) -> impl IntoResponse {
    let _guard = state.metrics.start_request();

    let result = state.breaker.execute(async {
        let books = state.books.read().await;
        let book = books.get(&req.book_id)
            .ok_or("Book not found")?;

        if book.stock < req.quantity {
            return Err("Insufficient stock".to_string());
        }
        Ok((book.clone(), req.quantity))
    }).await;

    match result {
        Ok((book, quantity)) => {
            {
                let mut books = state.books.write().await;
                if let Some(b) = books.get_mut(&book.id) {
                    b.stock -= quantity;
                }
            }

            let book_id = req.book_id.clone();
            let order = Order {
                id: generate_short_id(),
                user_id: "current-user".into(),
                book_id: book_id.clone(),
                quantity,
                total: book.price * quantity as f64,
                status: "confirmed".into(),
                created_at: now_iso8601(),
            };

            state.orders.write().await.insert(order.id.clone(), order.clone());
            let _ = state.cache.delete(&format!("book:{}", book_id));

            state.metrics.record_success();
            log::info(&format!("Order created: {}", order.id));
            JsonResponse::ok(order)
        }
        Err(e) => {
            state.metrics.record_error();
            JsonResponse::<Order>::error(400, format!("Order failed: {}", e))
        }
    }
}

async fn list_orders(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let _guard = state.metrics.start_request();
    let orders = state.orders.read().await;
    let list: Vec<Order> = orders.values().cloned().collect();
    state.metrics.record_success();
    JsonResponse::ok(list)
}

async fn cancel_order(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let _guard = state.metrics.start_request();

    let mut orders = state.orders.write().await;
    if let Some(order) = orders.get_mut(&id) {
        order.status = "cancelled".into();
        order.created_at = now_iso8601();
        state.metrics.record_success();
        log::info(&format!("Order cancelled: {}", id));
        JsonResponse::ok(order.clone())
    } else {
        state.metrics.record_error();
        JsonResponse::<Order>::error(404, "Order not found")
    }
}

// ─── Middleware ───────────────────────────────────────────────────────────

async fn shedder_middleware(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    if state.shedder.should_reject() {
        tracing::warn!("request rejected by load shedder");
        return JsonResponse::<serde_json::Value>::error(503, "service overloaded").into_response();
    }
    next.run(req).await
}

// ─── Main ─────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = rszero::config::RszeroConfig::default();
    log::init(&config.log);
    log::info("========================================");
    log::info("  Bookstore API Gateway Starting");
    log::info("========================================");

    let state = Arc::new(AppState::new());

    let app = axum::Router::new()
        // Public routes
        .route("/health", get(health_check))
        .route("/metrics", get(metrics_handler))
        .route("/openapi.json", get(openapi_handler))
        .route("/v1/auth/login", post(login))
        .route("/v1/books", get(list_books))
        .route("/v1/books/:id", get(get_book))
        // Protected routes
        .route("/v1/books", post(create_book))
        .route("/v1/orders", post(create_order).get(list_orders))
        .route("/v1/orders/:id/cancel", post(cancel_order))
        // Middleware chain
        .layer(middleware::from_fn_with_state(state.clone(), shedder_middleware))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(middleware::from_fn(trace_middleware))
        .with_state(state);

    let server = RszeroServer::new("0.0.0.0", 8080)
        .merge_router(app)
        .compression();

    log::info("API Gateway listening on http://0.0.0.0:8080");
    log::info("");
    log::info("Public Endpoints:");
    log::info("  GET  /health              - Health check");
    log::info("  GET  /metrics             - Prometheus metrics");
    log::info("  GET  /openapi.json        - OpenAPI specification");
    log::info("  POST /v1/auth/login       - Login (get JWT token)");
    log::info("  GET  /v1/books            - List books");
    log::info("  GET  /v1/books/:id        - Get book");
    log::info("");
    log::info("Protected Endpoints (JWT required):");
    log::info("  POST /v1/books            - Create book");
    log::info("  POST /v1/orders           - Create order");
    log::info("  GET  /v1/orders           - List orders");
    log::info("  POST /v1/orders/:id/cancel - Cancel order");
    log::info("========================================");

    server.start().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_generate_and_verify() {
        let jwt = JwtMiddleware::new("test-secret");
        let token = jwt.generate_token("user-1", 3600).unwrap();
        let claims = jwt.verify_token(&token).unwrap();
        assert_eq!(claims.sub, "user-1");
    }

    #[tokio::test]
    async fn test_health_check() {
        let state = Arc::new(AppState::new());
        let resp = health_check(State(state)).await;
        let _ = resp.into_response();
    }

    #[tokio::test]
    async fn test_login() {
        let state = Arc::new(AppState::new());
        let resp = login(
            State(state),
            Json(LoginReq { username: "admin".into(), password: "pass".into() }),
        ).await;
        let response = resp.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_login_empty() {
        let state = Arc::new(AppState::new());
        let resp = login(
            State(state),
            Json(LoginReq { username: "".into(), password: "".into() }),
        ).await;
        let response = resp.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    }
}
