//! OpenAPI specification generation helper.
//!
//! Provides types and builders for generating OpenAPI 3.0 specs from route definitions.

pub mod types;

use serde::Serialize;
use types::{ApiOperation, SecurityScheme};

/// OpenAPI 3.0 specification builder.
#[derive(Debug, Clone, Serialize)]
pub struct OpenApiSpec {
    openapi: String,
    info: ApiInfo,
    paths: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    components: Option<serde_json::Value>,
}

impl Default for OpenApiSpec {
    fn default() -> Self {
        Self {
            openapi: "3.0.0".into(),
            info: ApiInfo::default(),
            paths: serde_json::json!({}),
            components: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
struct ApiInfo {
    title: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

impl OpenApiSpec {
    /// Create a new OpenAPI spec builder.
    pub fn new(title: &str, version: &str) -> Self {
        Self {
            openapi: "3.0.0".into(),
            info: ApiInfo {
                title: title.into(),
                version: version.into(),
                description: None,
            },
            paths: serde_json::json!({}),
            components: None,
        }
    }

    /// Set the API description.
    pub fn description(mut self, desc: &str) -> Self {
        self.info.description = Some(desc.into());
        self
    }

    /// Add a path to the spec.
    pub fn path(mut self, path: &str, method: &str, operation: ApiOperation) -> Self {
        let paths = self.paths.as_object_mut()
            .expect("paths is always initialized as a JSON object");
        let path_obj = paths.entry(path.to_string())
            .or_insert_with(|| serde_json::json!({}));
        let path_map = path_obj.as_object_mut()
            .expect("path object is always a JSON object");
        let op_value = serde_json::to_value(&operation)
            .expect("ApiOperation serializes to JSON");
        path_map.insert(method.to_lowercase(), op_value);
        self
    }

    /// Add security scheme.
    pub fn security_scheme(mut self, name: &str, scheme: SecurityScheme) -> Self {
        let components = self.components.get_or_insert_with(|| serde_json::json!({}));
        let components_map = components.as_object_mut()
            .expect("components is always a JSON object");
        let security_schemes = components_map.entry("securitySchemes")
            .or_insert_with(|| serde_json::json!({}));
        let schemes_map = security_schemes.as_object_mut()
            .expect("securitySchemes is always a JSON object");
        let scheme_value = serde_json::to_value(&scheme)
            .expect("SecurityScheme serializes to JSON");
        schemes_map.insert(name.to_string(), scheme_value);
        self
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Serialize to YAML string.
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openapi_spec_basic() {
        let spec = OpenApiSpec::new("Test API", "1.0.0")
            .description("A test API")
            .path("/users", "GET", ApiOperation::new("List users").tag("users"));

        let json = spec.to_json().unwrap();
        assert!(json.contains("Test API"));
        assert!(json.contains("3.0.0"));
        assert!(json.contains("/users"));
    }

    #[test]
    fn test_openapi_spec_with_params() {
        let spec = OpenApiSpec::new("Test API", "1.0.0")
            .path("/users/{id}", "GET",
                ApiOperation::new("Get user")
                    .path_param("id", "string")
                    .query_param("fields", "string")
            );

        let json = spec.to_json().unwrap();
        assert!(json.contains("path"));
        assert!(json.contains("query"));
    }

    #[test]
    fn test_openapi_spec_with_security() {
        let spec = OpenApiSpec::new("Test API", "1.0.0")
            .security_scheme("BearerAuth", SecurityScheme::jwt());

        let json = spec.to_json().unwrap();
        assert!(json.contains("securitySchemes"));
        assert!(json.contains("bearer"));
        assert!(json.contains("JWT"));
    }
}
