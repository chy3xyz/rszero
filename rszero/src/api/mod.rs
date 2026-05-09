//! go-zero `.api` file parser — lexer + AST.
//!
//! Parses go-zero's `.api` definition files into an AST for code generation.
//! Supports `syntax`, `info`, `type`, `service`, `handler`, and route declarations.
//!
//! # Supported Syntax
//! ```ignore
//! syntax = "v1"
//!
//! info (
//!     title: "user api"
//!     desc: "user service"
//! )
//!
//! type User {
//!     Id   int64  `json:"id"`
//!     Name string `json:"name"`
//! }
//!
//! type GetUserReq {
//!     Id int64 `path:"id"`
//! }
//!
//! service user-api {
//!     @handler getUser
//!     get /users/:id (GetUserReq) returns (User)
//!
//!     @handler listUsers
//!     get /users returns (ListUserResp)
//! }
//! ```

pub mod lexer;
pub mod parser;
pub mod codegen;

pub use lexer::{Lexer, Token, TokenKind};
pub use parser::{ApiFile, ApiType, ApiField, ApiService, ApiRoute, RouteMethod, parse_api};
pub use codegen::{generate, GeneratedCode};

use crate::error::RszeroResult;

/// Parse a `.api` file from a string.
pub fn parse(source: &str) -> RszeroResult<ApiFile> {
    parse_api(source)
}

/// Parse a `.api` file from a file path.
pub fn parse_file(path: &str) -> RszeroResult<ApiFile> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| crate::error::RszeroError::Internal { message: format!("read api file failed: {}", e), source: None })?;
    parse(&source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_api() {
        let source = r#"
syntax = "v1"

info (
    title: "test api"
)

type User {
    Id   int64  `json:"id"`
}

service user-api {
    @handler getUser
    get /users/:id (GetUserReq) returns (User)
}
"#;
        let api = parse(source).unwrap();
        assert_eq!(api.syntax, "v1");
        assert_eq!(api.types.len(), 1);
        assert_eq!(api.services.len(), 1);
        assert_eq!(api.services[0].routes.len(), 1);
    }

    #[test]
    fn test_parse_file_not_found() {
        let result = parse_file("/nonexistent/test.api");
        assert!(result.is_err());
    }
}
