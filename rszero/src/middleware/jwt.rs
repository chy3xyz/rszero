//! JWT authentication middleware.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::http::StatusCode;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use crate::error::RszeroError;

/// JWT claims for authentication.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user identifier).
    pub sub: String,
    /// Expiration timestamp (Unix seconds).
    pub exp: usize,
}

/// JWT authentication middleware with token generation and verification.
#[derive(Clone)]
pub struct JwtMiddleware {
    secret: String,
}

impl JwtMiddleware {
    /// Create a new middleware with the given signing secret.
    pub fn new(secret: impl Into<String>) -> Self {
        Self { secret: secret.into() }
    }

    /// Generate a JWT token for the given subject with expiration in seconds.
    pub fn generate_token(&self, subject: &str, expiry_secs: usize) -> Result<String, RszeroError> {
        let claims = Claims {
            sub: subject.to_string(),
            exp: (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
                .as_secs() as usize) + expiry_secs,
        };
        encode(&Header::default(), &claims, &EncodingKey::from_secret(self.secret.as_bytes()))
            .map_err(|e| RszeroError::Auth { message: e.to_string(), source: None })
    }

    /// Verify a JWT token and return the claims.
    pub fn verify_token(&self, token: &str) -> Result<Claims, RszeroError> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        ).map_err(|e| RszeroError::Auth { message: e.to_string(), source: None })?;
        Ok(token_data.claims)
    }

    /// Axum middleware that validates the Authorization header.
    pub async fn middleware(&self, req: Request, next: Next) -> Response {
        let auth_header = req.headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());

        match auth_header {
            Some(header) => {
                let token = header.strip_prefix("Bearer ").unwrap_or(header);
                match self.verify_token(token) {
                    Ok(_claims) => next.run(req).await,
                    Err(_) => {
                        let mut res = Response::new(axum::body::Body::from(
                            serde_json::json!({"code": 401, "msg": "unauthorized"}).to_string(),
                        ));
                        *res.status_mut() = StatusCode::UNAUTHORIZED;
                        res
                    }
                }
            }
            None => {
                let mut res = Response::new(axum::body::Body::from(
                    serde_json::json!({"code": 401, "msg": "missing authorization"}).to_string(),
                ));
                *res.status_mut() = StatusCode::UNAUTHORIZED;
                res
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify() {
        let mw = JwtMiddleware::new("test-secret");
        let token = mw.generate_token("user-123", 3600).unwrap();
        let claims = mw.verify_token(&token).unwrap();
        assert_eq!(claims.sub, "user-123");
    }

    #[test]
    fn test_verify_invalid_token() {
        let mw = JwtMiddleware::new("secret-a");
        let token = mw.generate_token("user", 3600).unwrap();
        let mw2 = JwtMiddleware::new("secret-b");
        assert!(mw2.verify_token(&token).is_err());
    }

    #[test]
    fn test_verify_malformed_token() {
        let mw = JwtMiddleware::new("secret");
        assert!(mw.verify_token("not-a-jwt").is_err());
    }
}
