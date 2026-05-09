//! go-zero httpx compatibility layer.
//!
//! Provides `ok()` and `error()` response helpers.

use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

#[derive(Serialize)]
struct SuccessResponse<T: Serialize> { code: i32, msg: String, data: T }

#[derive(Serialize)]
struct FailResponse { code: i32, msg: String }

/// Create a success response.
pub fn ok<T: Serialize>(data: T) -> impl IntoResponse {
    (StatusCode::OK, Json(SuccessResponse { code: 0, msg: "ok".into(), data }))
}

/// Create an error response with the given code and message.
pub fn error(code: i32, msg: impl Into<String>) -> impl IntoResponse {
    let status = if code >= 500 {
        StatusCode::INTERNAL_SERVER_ERROR
    } else if code >= 400 {
        StatusCode::from_u16(code as u16).unwrap_or(StatusCode::BAD_REQUEST)
    } else {
        StatusCode::OK
    };
    (status, Json(FailResponse { code, msg: msg.into() }))
}
