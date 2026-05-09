//! Multi-tenant middleware for extracting and propagating tenant context.
//!
//! Extracts `tenant_id` from headers or path parameters and injects it
//! into the request extensions for downstream handlers.
//!
//! # Example
//! ```ignore
//! use rszero::middleware::tenant::{TenantConfig, tenant_middleware};
//!
//! let config = TenantConfig::header("X-Tenant-ID");
//! ```

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Tenant extraction configuration.
#[derive(Debug, Clone)]
pub struct TenantConfig {
    /// Header name to extract tenant from.
    pub header_name: Option<String>,
    /// Path parameter name to extract tenant from.
    pub path_param: Option<String>,
    /// Default tenant if none is provided.
    pub default_tenant: Option<String>,
}

impl TenantConfig {
    /// Extract tenant from a header.
    pub fn header(name: &str) -> Self {
        Self {
            header_name: Some(name.to_string()),
            path_param: None,
            default_tenant: None,
        }
    }

    /// Extract tenant from a path parameter.
    pub fn path_param(name: &str) -> Self {
        Self {
            header_name: None,
            path_param: Some(name.to_string()),
            default_tenant: None,
        }
    }

    /// Set a default tenant.
    pub fn with_default(mut self, tenant: &str) -> Self {
        self.default_tenant = Some(tenant.to_string());
        self
    }
}

/// Tenant ID wrapper for request extensions.
#[derive(Debug, Clone)]
pub struct TenantId(pub String);

/// Extract tenant ID from a request.
pub fn extract_tenant(req: &Request, config: &TenantConfig) -> Option<String> {
    if let Some(ref header) = config.header_name {
        if let Some(val) = req.headers().get(header) {
            if let Ok(s) = val.to_str() {
                return Some(s.to_string());
            }
        }
    }

    if let Some(ref param) = config.path_param {
        let path = req.uri().path();
        // Simple path param extraction: /:tenant/... or /tenants/:id/...
        // For production, integrate with axum's MatchedPath extractor
        let parts: Vec<&str> = path.split('/').collect();
        if let Some(pos) = parts.iter().position(|p| *p == format!(":{}", param)) {
            if pos + 1 < parts.len() {
                return Some(parts[pos + 1].to_string());
            }
        }
    }

    config.default_tenant.clone()
}

/// Tenant middleware.
pub async fn tenant_middleware(
    config: std::sync::Arc<TenantConfig>,
    mut req: Request,
    next: Next,
) -> Response {
    let tenant_id = extract_tenant(&req, &config)
        .unwrap_or_else(|| "default".to_string());

    tracing::debug!(tenant_id = %tenant_id, "tenant extracted");
    req.extensions_mut().insert(TenantId(tenant_id));

    next.run(req).await
}

/// Tower-compatible wrapper.
pub struct TenantLayer;

impl TenantLayer {
    /// Create middleware function.
    pub fn middleware(
        config: std::sync::Arc<TenantConfig>,
    ) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
        move |req, next| {
            let config = config.clone();
            Box::pin(tenant_middleware(config, req, next))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tenant_from_header() {
        let req = Request::builder()
            .uri("/")
            .header("X-Tenant-ID", "acme")
            .body(axum::body::Body::empty())
            .unwrap();
        let config = TenantConfig::header("X-Tenant-ID");
        assert_eq!(extract_tenant(&req, &config).unwrap(), "acme");
    }

    #[test]
    fn test_extract_tenant_default() {
        let req = Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .unwrap();
        let config = TenantConfig::header("X-Tenant-ID")
            .with_default("default-tenant");
        assert_eq!(extract_tenant(&req, &config).unwrap(), "default-tenant");
    }

    #[test]
    fn test_tenant_id_wrapper() {
        let t = TenantId("acme".into());
        assert_eq!(t.0, "acme");
    }
}
