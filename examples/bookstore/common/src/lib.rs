//! Shared types and error definitions for bookstore services.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── Shared Error Types ───────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum BookstoreError {
    #[error("book not found: {0}")]
    BookNotFound(String),
    #[error("order not found: {0}")]
    OrderNotFound(String),
    #[error("insufficient stock: {book_id} has {available}, requested {requested}")]
    InsufficientStock { book_id: String, available: u32, requested: u32 },
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("order already cancelled: {0}")]
    OrderAlreadyCancelled(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl BookstoreError {
    pub fn code(&self) -> i32 {
        match self {
            Self::BookNotFound(_) => 404,
            Self::OrderNotFound(_) => 404,
            Self::InsufficientStock { .. } => 400,
            Self::InvalidInput(_) => 400,
            Self::OrderAlreadyCancelled(_) => 409,
            Self::Internal(_) => 500,
        }
    }
}

pub type BookstoreResult<T> = Result<T, BookstoreError>;

// ─── Pagination ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub page: i32,
    pub page_size: i32,
    pub total: i32,
}

impl Pagination {
    pub fn new(page: i32, page_size: i32, total: i32) -> Self {
        Self { page, page_size, total }
    }

    pub fn offset(&self) -> i32 {
        (self.page - 1) * self.page_size
    }

    pub fn total_pages(&self) -> i32 {
        (self.total + self.page_size - 1) / self.page_size
    }
}

// ─── API Response ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResp<T: Serialize> {
    pub code: i32,
    pub msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: Serialize> ApiResp<T> {
    pub fn ok(data: T) -> Self {
        Self { code: 0, msg: "ok".into(), data: Some(data) }
    }

    pub fn error(code: i32, msg: impl Into<String>) -> Self {
        Self { code, msg: msg.into(), data: None }
    }

    pub fn from_result(result: BookstoreResult<T>) -> Self {
        match result {
            Ok(data) => Self::ok(data),
            Err(e) => Self::error(e.code(), e.to_string()),
        }
    }
}
