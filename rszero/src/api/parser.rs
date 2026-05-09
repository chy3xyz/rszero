//! Parser for go-zero `.api` files.

use crate::error::{RszeroError, RszeroResult};
use super::lexer::{Token, TokenKind};

/// Parsed `.api` file AST.
#[derive(Debug, Clone)]
pub struct ApiFile {
    /// Syntax version (e.g. "v1").
    pub syntax: String,
    /// Info block metadata.
    pub info: Vec<(String, String)>,
    /// Type definitions.
    pub types: Vec<ApiType>,
    /// Service definitions.
    pub services: Vec<ApiService>,
}

/// Type definition.
#[derive(Debug, Clone)]
pub struct ApiType {
    /// Type name.
    pub name: String,
    /// Fields.
    pub fields: Vec<ApiField>,
}

/// Type field.
#[derive(Debug, Clone)]
pub struct ApiField {
    /// Field name.
    pub name: String,
    /// Field type.
    pub ty: String,
    /// Struct tags (e.g. `json:"id"`).
    pub tag: Option<String>,
}

/// Service definition.
#[derive(Debug, Clone)]
pub struct ApiService {
    /// Service name.
    pub name: String,
    /// Handlers / routes.
    pub routes: Vec<ApiRoute>,
}

/// Route definition.
#[derive(Debug, Clone)]
pub struct ApiRoute {
    /// Handler name (from @handler).
    pub handler: String,
    /// HTTP method.
    pub method: RouteMethod,
    /// Path pattern.
    pub path: String,
    /// Request type name (if any).
    pub request_type: Option<String>,
    /// Response type name (if any).
    pub response_type: Option<String>,
}

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMethod {
    /// GET
    Get,
    /// POST
    Post,
    /// PUT
    Put,
    /// DELETE
    Delete,
    /// PATCH
    Patch,
    /// HEAD
    Head,
}

impl RouteMethod {
    /// Convert to uppercase string.
    pub fn as_str(&self) -> &'static str {
        match self {
            RouteMethod::Get => "GET",
            RouteMethod::Post => "POST",
            RouteMethod::Put => "PUT",
            RouteMethod::Delete => "DELETE",
            RouteMethod::Patch => "PATCH",
            RouteMethod::Head => "HEAD",
        }
    }
}

/// Parser for `.api` tokens.
pub struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    /// Create a new parser from tokens.
    pub fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parse the entire file.
    pub fn parse(&mut self) -> RszeroResult<ApiFile> {
        let mut syntax = String::from("v1");
        let mut info = Vec::new();
        let mut types = Vec::new();
        let mut services = Vec::new();

        self.skip_newlines();

        while !self.is_eof() {
            if let Some(ident) = self.peek_ident() {
                match ident.as_str() {
                    "syntax" => {
                        syntax = self.parse_syntax()?;
                    }
                    "info" => {
                        info = self.parse_info()?;
                    }
                    "type" => {
                        types.push(self.parse_type_def()?);
                    }
                    "service" => {
                        services.push(self.parse_service()?);
                    }
                    _ => {
                        return Err(RszeroError::Internal { message: format!("unexpected keyword '{}' at line {}", ident, self.current_line()), source: None });
                    }
                }
            } else {
                self.advance();
            }
            self.skip_newlines();
        }

        Ok(ApiFile { syntax, info, types, services })
    }

    fn parse_syntax(&mut self) -> RszeroResult<String> {
        self.expect_ident("syntax")?;
        self.expect(TokenKind::Eq)?;
        let val = self.expect_string()?;
        Ok(val)
    }

    fn parse_info(&mut self) -> RszeroResult<Vec<(String, String)>> {
        self.expect_ident("info")?;
        self.expect(TokenKind::LParen)?;
        let mut pairs = Vec::new();
        self.skip_newlines();
        while !self.check(&TokenKind::RParen) && !self.is_eof() {
            if let Some(key) = self.peek_ident() {
                self.advance();
                self.expect(TokenKind::Colon)?;
                let value = self.expect_string()?;
                pairs.push((key, value));
            } else {
                self.advance();
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::RParen)?;
        Ok(pairs)
    }

    fn parse_type_def(&mut self) -> RszeroResult<ApiType> {
        self.expect_ident("type")?;
        let name = self.expect_ident_str()?;
        self.expect(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        self.skip_newlines();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            if let Some(field_name) = self.peek_ident() {
                self.advance();
                let ty = self.expect_ident_str()?;
                let tag = if self.check(&TokenKind::Backtick(String::new())) {
                    Some(self.expect_backtick()?)
                } else {
                    None
                };
                fields.push(ApiField { name: field_name, ty, tag });
            } else {
                self.advance();
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace)?;
        Ok(ApiType { name, fields })
    }

    fn parse_service(&mut self) -> RszeroResult<ApiService> {
        self.expect_ident("service")?;
        let name = self.expect_ident_str()?;
        self.expect(TokenKind::LBrace)?;
        let mut routes = Vec::new();
        self.skip_newlines();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            if self.check(&TokenKind::At) {
                routes.push(self.parse_route()?);
            } else {
                self.advance();
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace)?;
        Ok(ApiService { name, routes })
    }

    fn parse_route(&mut self) -> RszeroResult<ApiRoute> {
        self.expect(TokenKind::At)?;
        self.expect_ident("handler")?;
        let handler = self.expect_ident_str()?;
        self.skip_newlines();

        let method = match self.expect_ident_str()?.to_lowercase().as_str() {
            "get" => RouteMethod::Get,
            "post" => RouteMethod::Post,
            "put" => RouteMethod::Put,
            "delete" => RouteMethod::Delete,
            "patch" => RouteMethod::Patch,
            "head" => RouteMethod::Head,
            other => return Err(RszeroError::Internal { message: format!("unknown HTTP method: {}", other), source: None }),
        };

        let path = self.expect_path()?;

        let request_type = if self.check(&TokenKind::LParen) {
            self.expect(TokenKind::LParen)?;
            let name = self.expect_ident_str()?;
            self.expect(TokenKind::RParen)?;
            Some(name)
        } else {
            None
        };

        let response_type = if self.peek_ident().map(|s| s == "returns").unwrap_or(false) {
            self.advance();
            self.expect(TokenKind::LParen)?;
            let name = self.expect_ident_str()?;
            self.expect(TokenKind::RParen)?;
            Some(name)
        } else {
            None
        };

        Ok(ApiRoute { handler, method, path, request_type, response_type })
    }

    // ─── Helper methods ─────────────────────────────────────────────────────

    fn current_line(&self) -> usize {
        self.tokens.get(self.pos).map(|t| t.line).unwrap_or(0)
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.tokens[self.pos].kind, TokenKind::Eof)
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_ident(&self) -> Option<String> {
        match self.peek() {
            Some(Token { kind: TokenKind::Ident(s), .. }) => Some(s.clone()),
            _ => None,
        }
    }

    fn check(&self, kind: &TokenKind) -> bool {
        match self.peek() {
            Some(t) => std::mem::discriminant(&t.kind) == std::mem::discriminant(kind),
            None => false,
        }
    }

    fn skip_newlines(&mut self) {
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
    }

    fn expect(&mut self, kind: TokenKind) -> RszeroResult<()> {
        if self.check(&kind) {
            self.advance();
            Ok(())
        } else {
            Err(RszeroError::Internal { message: format!(
                "expected {:?} at line {}, got {:?}",
                kind, self.current_line(), self.peek()
            ), source: None })
        }
    }

    fn expect_ident(&mut self, expected: &str) -> RszeroResult<()> {
        match self.peek_ident() {
            Some(ref s) if s == expected => {
                self.advance();
                Ok(())
            }
            _ => Err(RszeroError::Internal { message: format!(
                "expected '{}' at line {}, got {:?}",
                expected, self.current_line(), self.peek()
            ), source: None }),
        }
    }

    fn expect_ident_str(&mut self) -> RszeroResult<String> {
        match self.peek_ident() {
            Some(s) => {
                self.advance();
                Ok(s)
            }
            None => Err(RszeroError::Internal { message: format!(
                "expected identifier at line {}, got {:?}",
                self.current_line(), self.peek()
            ), source: None }),
        }
    }

    fn expect_string(&mut self) -> RszeroResult<String> {
        match self.peek() {
            Some(Token { kind: TokenKind::String(s), .. }) => {
                let val = s.clone();
                self.advance();
                Ok(val)
            }
            _ => Err(RszeroError::Internal { message: format!(
                "expected string at line {}, got {:?}",
                self.current_line(), self.peek()
            ), source: None }),
        }
    }

    fn expect_backtick(&mut self) -> RszeroResult<String> {
        match self.peek() {
            Some(Token { kind: TokenKind::Backtick(s), .. }) => {
                let val = s.clone();
                self.advance();
                Ok(val)
            }
            _ => Err(RszeroError::Internal { message: format!(
                "expected backtick string at line {}, got {:?}",
                self.current_line(), self.peek()
            ), source: None }),
        }
    }

    fn expect_path(&mut self) -> RszeroResult<String> {
        // Path can be: /users/:id or /users
        let mut path = String::new();
        while let Some(t) = self.peek() {
            match &t.kind {
                TokenKind::Ident(s) => { path.push_str(s); self.advance(); }
                TokenKind::Number(s) => { path.push_str(s); self.advance(); }
                TokenKind::Colon => { path.push(':'); self.advance(); }
                TokenKind::Slash => { path.push('/'); self.advance(); }
                TokenKind::Unknown('/') => { path.push('/'); self.advance(); }
                TokenKind::LParen | TokenKind::Newline | TokenKind::Eof => break,
                _ => break,
            }
        }
        if path.is_empty() {
            return Err(RszeroError::Internal { message: format!(
                "expected path at line {}", self.current_line()
            ), source: None });
        }
        Ok(path)
    }
}

/// Convenience function to parse `.api` source.
pub fn parse_api(source: &str) -> RszeroResult<ApiFile> {
    use super::lexer::Lexer;
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(&tokens);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_syntax() {
        let source = r#"syntax = "v1""#;
        let api = parse_api(source).unwrap();
        assert_eq!(api.syntax, "v1");
    }

    #[test]
    fn test_parser_type() {
        let source = r#"
type User {
    Id   int64  `json:"id"`
    Name string `json:"name"`
}
"#;
        let api = parse_api(source).unwrap();
        assert_eq!(api.types.len(), 1);
        assert_eq!(api.types[0].name, "User");
        assert_eq!(api.types[0].fields.len(), 2);
    }

    #[test]
    fn test_parser_service() {
        let source = r#"
service user-api {
    @handler getUser
    get /users/:id (GetUserReq) returns (User)

    @handler listUsers
    get /users returns (ListUserResp)
}
"#;
        let api = parse_api(source).unwrap();
        assert_eq!(api.services.len(), 1);
        assert_eq!(api.services[0].routes.len(), 2);
        assert_eq!(api.services[0].routes[0].handler, "getUser");
        assert_eq!(api.services[0].routes[0].method, RouteMethod::Get);
        assert_eq!(api.services[0].routes[0].path, "/users/:id");
    }
}
