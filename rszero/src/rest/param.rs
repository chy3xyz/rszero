//! Request parameter binding helpers for Axum.
//!
//! Provides convenience extractors and validators for path, query, and form parameters,
//! replicating go-zero's `path:"id"`, `form:"name"` tag-based binding.

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::de::DeserializeOwned;
use std::collections::HashMap;

/// Extract and validate path parameters.
///
/// # Example
/// ```ignore
/// async fn get_user(PathParam(id): PathParam<i64>) -> impl IntoResponse {
///     format!("user id: {}", id)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct PathParam<T>(pub T);

#[async_trait::async_trait]
impl<T, S> axum::extract::FromRequestParts<S> for PathParam<T>
where
    T: DeserializeOwned + Send + Sync,
    S: Send + Sync,
{
    type Rejection = ParamError;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, state: &S) -> Result<Self, Self::Rejection> {
        let path = Path::<HashMap<String, String>>::from_request_parts(parts, state).await
            .map_err(|e| ParamError::Path(e.to_string()))?;
        let value: T = serde_json::from_value(serde_json::to_value(&path.0)
            .map_err(|e| ParamError::Parse(e.to_string()))?)
            .map_err(|e| ParamError::Parse(e.to_string()))?;
        Ok(PathParam(value))
    }
}

/// Extract and validate query parameters.
///
/// # Example
/// ```ignore
/// async fn list_users(QueryParam(params): QueryParam<ListParams>) -> impl IntoResponse {
///     format!("page: {:?}", params.page)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct QueryParam<T>(pub T);

#[async_trait::async_trait]
impl<T, S> axum::extract::FromRequestParts<S> for QueryParam<T>
where
    T: DeserializeOwned + Send + Sync,
    S: Send + Sync,
{
    type Rejection = ParamError;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, state: &S) -> Result<Self, Self::Rejection> {
        let query = Query::<HashMap<String, String>>::from_request_parts(parts, state).await
            .map_err(|e| ParamError::Query(e.to_string()))?;
        let value: T = serde_json::from_value(serde_json::to_value(&query.0)
            .map_err(|e| ParamError::Parse(e.to_string()))?)
            .map_err(|e| ParamError::Parse(e.to_string()))?;
        Ok(QueryParam(value))
    }
}

/// Parameter extraction error.
#[derive(Debug, Clone)]
pub enum ParamError {
    /// Path parameter extraction failed.
    Path(String),
    /// Query parameter extraction failed.
    Query(String),
    /// Parsing failed.
    Parse(String),
    /// Validation failed.
    Validation(String),
}

impl IntoResponse for ParamError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            ParamError::Path(m) => (StatusCode::BAD_REQUEST, format!("path param error: {}", m)),
            ParamError::Query(m) => (StatusCode::BAD_REQUEST, format!("query param error: {}", m)),
            ParamError::Parse(m) => (StatusCode::BAD_REQUEST, format!("parse error: {}", m)),
            ParamError::Validation(m) => (StatusCode::BAD_REQUEST, format!("validation error: {}", m)),
        };
        let body = serde_json::json!({ "code": 400, "msg": msg });
        (status, axum::Json(body)).into_response()
    }
}

/// Validate a string parameter is non-empty.
pub fn validate_required(value: &str, name: &str) -> Result<(), ParamError> {
    if value.trim().is_empty() {
        Err(ParamError::Validation(format!("{} is required", name)))
    } else {
        Ok(())
    }
}

/// Validate a numeric range.
pub fn validate_range<T>(value: T, name: &str, min: T, max: T) -> Result<(), ParamError>
where
    T: PartialOrd + std::fmt::Display,
{
    if value < min || value > max {
        Err(ParamError::Validation(format!(
            "{} must be between {} and {}", name, min, max
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_required() {
        assert!(validate_required("hello", "name").is_ok());
        assert!(validate_required("", "name").is_err());
        assert!(validate_required("  ", "name").is_err());
    }

    #[test]
    fn test_validate_range() {
        assert!(validate_range(5i32, "age", 0, 120).is_ok());
        assert!(validate_range(-1i32, "age", 0, 120).is_err());
        assert!(validate_range(200i32, "age", 0, 120).is_err());
    }

    #[test]
    fn test_param_error_response() {
        let err = ParamError::Validation("test".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
