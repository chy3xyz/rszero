//! Request/response types for go-zero compatible JSON responses.

use serde::Serialize;
use axum::response::IntoResponse;
use axum::Json;
use axum::http::StatusCode;

/// Go-zero compatible JSON response: `{ "code": N, "msg": "...", "data": ... }`.
#[derive(Debug, Serialize)]
pub struct JsonResponse<T: Serialize> {
    /// Status code (0 = success).
    pub code: i32,
    /// Human-readable message.
    pub msg: String,
    /// Response payload (omitted if None).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl JsonResponse<()> {
    /// Create an empty success response.
    pub fn ok_empty() -> Self {
        Self { code: 0, msg: "ok".into(), data: None }
    }
}

impl<T: Serialize> JsonResponse<T> {
    /// Create a success response with data.
    pub fn ok(data: T) -> Self {
        Self { code: 0, msg: "ok".into(), data: Some(data) }
    }

    /// Create an error response (data is None).
    pub fn error(code: i32, msg: impl Into<String>) -> Self {
        Self { code, msg: msg.into(), data: None }
    }
}

impl<T: Serialize> IntoResponse for JsonResponse<T> {
    fn into_response(self) -> axum::response::Response {
        let status = if self.code == 0 {
            StatusCode::OK
        } else if self.code >= 500 {
            StatusCode::INTERNAL_SERVER_ERROR
        } else {
            StatusCode::from_u16(self.code as u16).unwrap_or(StatusCode::BAD_REQUEST)
        };
        (status, Json(self)).into_response()
    }
}

/// Convenience: create a success response.
pub fn ok_response<T: Serialize>(data: T) -> JsonResponse<T> { JsonResponse::ok(data) }

/// Convenience: create an error response.
pub fn error_response(code: i32, msg: impl Into<String>) -> JsonResponse<()> { JsonResponse::error(code, msg) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_response_ok() {
        let resp = JsonResponse::ok("hello");
        assert_eq!(resp.code, 0);
        assert_eq!(resp.data, Some("hello"));
    }

    #[test]
    fn test_json_response_error() {
        let resp: JsonResponse<()> = JsonResponse::error(404, "not found");
        assert_eq!(resp.code, 404);
        assert!(resp.data.is_none());
    }

    #[test]
    fn test_json_response_serialize() {
        let resp = JsonResponse::ok(42);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"code\":0"));
        assert!(json.contains("\"data\":42"));
    }
}
