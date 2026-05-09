//! Book RPC Service — Full CRUD with caching, circuit breaker, and service discovery.
//!
//! Demonstrates rszero best practices:
//! - Service context pattern (SVC)
//! - Cache-aside with TTL
//! - Circuit breaker for downstream calls
//! - Structured logging with tracing
//! - Unified error handling

use rszero::prelude::*;
use bookstore_common::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;

// ─── Domain Models ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub title: String,
    pub author: String,
    pub price: f64,
    pub stock: u32,
    pub isbn: String,
    pub category: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBookReq {
    pub title: String,
    pub author: String,
    pub price: f64,
    pub isbn: String,
    pub category: String,
    pub stock: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBookReq {
    pub id: String,
    pub title: String,
    pub author: String,
    pub price: f64,
    pub stock: u32,
}

// ─── Service Context (SVC Pattern) ────────────────────────────────────────

/// Service context — holds all dependencies for the book service.
/// This is the go-zero "svc" pattern: a single struct that carries
/// config, cache, database, and other shared resources.
pub struct BookSvc {
    pub config: RszeroConfig,
    pub book_cache: MemCache<String, Book>,
    pub list_cache: MemCache<String, Vec<Book>>,
    pub db: RwLock<HashMap<String, Book>>,
    pub breaker: CircuitBreaker,
}

impl BookSvc {
    pub fn new(config: RszeroConfig) -> Self {
        Self {
            book_cache: MemCache::new(500),
            list_cache: MemCache::new(100),
            db: RwLock::new(Self::seed_data()),
            breaker: CircuitBreaker::new(5),
            config,
        }
    }

    fn seed_data() -> HashMap<String, Book> {
        let now = chrono::Utc::now().timestamp();
        let mut books = HashMap::new();
        books.insert("book-1".into(), Book {
            id: "book-1".into(), title: "The Rust Programming Language".into(),
            author: "Steve Klabnik".into(), price: 39.99, stock: 100,
            isbn: "978-1718500440".into(), category: "programming".into(),
            created_at: now, updated_at: now,
        });
        books.insert("book-2".into(), Book {
            id: "book-2".into(), title: "Programming Rust".into(),
            author: "Jim Blandy".into(), price: 49.99, stock: 50,
            isbn: "978-1492052593".into(), category: "programming".into(),
            created_at: now, updated_at: now,
        });
        books.insert("book-3".into(), Book {
            id: "book-3".into(), title: "Designing Data-Intensive Applications".into(),
            author: "Martin Kleppmann".into(), price: 44.99, stock: 75,
            isbn: "978-1449373320".into(), category: "architecture".into(),
            created_at: now, updated_at: now,
        });
        books
    }
}

// ─── Logic Layer ──────────────────────────────────────────────────────────

/// Book logic — business operations with caching and circuit breaker.
pub struct BookLogic {
    svc: Arc<BookSvc>,
}

impl BookLogic {
    pub fn new(svc: Arc<BookSvc>) -> Self { Self { svc } }

    /// Get a book by ID — uses cache-aside pattern with circuit breaker.
    pub async fn get_book(&self, id: &str) -> BookstoreResult<Book> {
        let cache_key = format!("book:{}", id);

        // 1. Try cache
        if let Some(book) = self.svc.book_cache.get(&cache_key) {
            tracing::debug!("cache hit: {}", cache_key);
            return Ok(book);
        }

        // 2. Cache miss — use circuit breaker for DB call
        let book = self.svc.breaker.execute(async {
            let db = self.svc.db.read().await;
            db.get(id).cloned().ok_or_else(|| BookstoreError::BookNotFound(id.to_string()))
        }).await.map_err(|e| BookstoreError::Internal(e.to_string()))?;

        // 3. Write cache with TTL
        let _ = self.svc.book_cache.set_with_ttl(cache_key, book.clone(), Some(std::time::Duration::from_secs(300)));
        Ok(book)
    }

    /// List books with pagination and category filter.
    pub async fn list_books(&self, category: Option<&str>, page: i32, page_size: i32) -> BookstoreResult<(Vec<Book>, i32)> {
        let cache_key = match category {
            Some(cat) => format!("books:cat:{}:p{}", cat, page),
            None => format!("books:all:p{}", page),
        };

        if let Some(books) = self.svc.list_cache.get(&cache_key) {
            let total = self.svc.db.read().await.len() as i32;
            return Ok((books, total));
        }

        let db = self.svc.db.read().await;
        let mut books: Vec<Book> = db.values()
            .filter(|b| category.map_or(true, |c| b.category == c))
            .cloned()
            .collect();

        let total = books.len() as i32;
        let offset = ((page - 1) * page_size) as usize;
        let end = (offset + page_size as usize).min(books.len());
        let page_books = if offset < books.len() {
            books.drain(offset..end).collect()
        } else {
            Vec::new()
        };

        drop(db);
        let _ = self.svc.list_cache.set_with_ttl(cache_key, page_books.clone(), Some(std::time::Duration::from_secs(60)));
        Ok((page_books, total))
    }

    /// Create a new book.
    pub async fn create_book(&self, req: CreateBookReq) -> BookstoreResult<Book> {
        if req.title.is_empty() {
            return Err(BookstoreError::InvalidInput("title is required".into()));
        }
        if req.price <= 0.0 {
            return Err(BookstoreError::InvalidInput("price must be positive".into()));
        }

        let now = chrono::Utc::now().timestamp();
        let book = Book {
            id: generate_short_id(),
            title: req.title,
            author: req.author,
            price: req.price,
            stock: req.stock,
            isbn: req.isbn,
            category: req.category,
            created_at: now,
            updated_at: now,
        };

        let mut db = self.svc.db.write().await;
        db.insert(book.id.clone(), book.clone());

        // Invalidate list cache
        self.svc.book_cache.clear();
        self.svc.list_cache.clear();

        tracing::info!(book_id = %book.id, title = %book.title, "book created");
        Ok(book)
    }

    /// Update an existing book.
    pub async fn update_book(&self, req: UpdateBookReq) -> BookstoreResult<Book> {
        let mut db = self.svc.db.write().await;
        let book = db.get_mut(&req.id)
            .ok_or_else(|| BookstoreError::BookNotFound(req.id.clone()))?;

        book.title = req.title;
        book.author = req.author;
        book.price = req.price;
        book.stock = req.stock;
        book.updated_at = chrono::Utc::now().timestamp();

        let updated = book.clone();
        drop(db);

        // Update cache
        let cache_key = format!("book:{}", req.id);
        let _ = self.svc.book_cache.set_with_ttl(cache_key, updated.clone(), Some(std::time::Duration::from_secs(300)));
        self.svc.book_cache.clear();
        self.svc.list_cache.clear();

        tracing::info!(book_id = %updated.id, "book updated");
        Ok(updated)
    }

    /// Delete a book.
    pub async fn delete_book(&self, id: &str) -> BookstoreResult<bool> {
        let mut db = self.svc.db.write().await;
        let removed = db.remove(id).is_some();
        drop(db);

        if removed {
            let cache_key = format!("book:{}", id);
            let _ = self.svc.book_cache.delete(&cache_key);
            self.svc.book_cache.clear();
        self.svc.list_cache.clear();
            tracing::info!(book_id = %id, "book deleted");
        }

        Ok(removed)
    }

    /// Deduct stock — used by order service via RPC.
    pub async fn deduct_stock(&self, book_id: &str, quantity: u32) -> BookstoreResult<()> {
        let result = self.svc.breaker.execute(async {
            let mut db = self.svc.db.write().await;
            let book = db.get_mut(book_id)
                .ok_or_else(|| BookstoreError::BookNotFound(book_id.to_string()))?;

            if book.stock < quantity {
                return Err(BookstoreError::InsufficientStock {
                    book_id: book_id.to_string(),
                    available: book.stock,
                    requested: quantity,
                });
            }

            book.stock -= quantity;
            book.updated_at = chrono::Utc::now().timestamp();

            let cache_key = format!("book:{}", book_id);
            let _ = self.svc.book_cache.delete(&cache_key);
            self.svc.book_cache.clear();
            self.svc.list_cache.clear();

            tracing::info!(book_id = %book_id, quantity, "stock deducted");
            Ok::<_, BookstoreError>(())
        }).await;

        result.map_err(|e| BookstoreError::Internal(e.to_string()))
    }
}

// ─── RPC Service Implementation ───────────────────────────────────────────

/// Book RPC service — exposes methods for API gateway to call.
pub struct BookRpcService {
    logic: BookLogic,
}

impl BookRpcService {
    pub fn new(svc: Arc<BookSvc>) -> Self {
        Self { logic: BookLogic::new(svc) }
    }

    pub fn logic(&self) -> &BookLogic { &self.logic }
}

// ─── Main ─────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = load_config("etc/book-rpc.yaml")?;
    log::init(&config.log);
    log::info("Starting Book RPC service...");

    let svc = Arc::new(BookSvc::new(config));
    let rpc_service = BookRpcService::new(svc);

    // Register with service discovery
    let discovery = ServiceDiscovery::from_etcd(vec!["127.0.0.1:2379".into()]);
    discovery.register("book.rpc", "127.0.0.1:8081").await?;

    log::info("Book RPC service ready on :8081");
    log::info("  GetBook        - Get book by ID");
    log::info("  ListBooks      - List books with pagination");
    log::info("  CreateBook     - Create a new book");
    log::info("  UpdateBook     - Update book info");
    log::info("  DeleteBook     - Delete a book");
    log::info("  DeductStock    - Deduct stock (for orders)");

    // In production, this would start a Volo gRPC/Thrift server
    // For now, keep running
    tokio::signal::ctrl_c().await?;
    log::info("Book RPC service shutting down...");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_svc() -> Arc<BookSvc> {
        Arc::new(BookSvc::new(RszeroConfig::default()))
    }

    #[tokio::test]
    async fn test_get_book_cache_miss() {
        let svc = test_svc();
        let logic = BookLogic::new(svc);
        let book = logic.get_book("book-1").await.unwrap();
        assert_eq!(book.title, "The Rust Programming Language");
    }

    #[tokio::test]
    async fn test_get_book_not_found() {
        let svc = test_svc();
        let logic = BookLogic::new(svc);
        let result = logic.get_book("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_book() {
        let svc = test_svc();
        let logic = BookLogic::new(svc);
        let book = logic.create_book(CreateBookReq {
            title: "Test Book".into(),
            author: "Test Author".into(),
            price: 19.99,
            isbn: "000-0000000000".into(),
            category: "test".into(),
            stock: 10,
        }).await.unwrap();
        assert_eq!(book.title, "Test Book");
        assert!(!book.id.is_empty());
    }

    #[tokio::test]
    async fn test_deduct_stock() {
        let svc = test_svc();
        let logic = BookLogic::new(svc);
        logic.deduct_stock("book-1", 5).await.unwrap();
        let book = logic.get_book("book-1").await.unwrap();
        assert_eq!(book.stock, 95);
    }

    #[tokio::test]
    async fn test_deduct_stock_insufficient() {
        let svc = test_svc();
        let logic = BookLogic::new(svc);
        let result = logic.deduct_stock("book-1", 9999).await;
        assert!(result.is_err());
    }
}
