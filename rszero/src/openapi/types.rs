//! OpenAPI 3.0 type definitions.

use serde::Serialize;

/// API operation definition.
#[derive(Debug, Clone, Serialize)]
pub struct ApiOperation {
    /// Operation summary.
    pub summary: String,
    /// Operation description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Operation tags for grouping.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Operation parameters.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    /// Operation responses.
    pub responses: serde_json::Value,
}

impl ApiOperation {
    /// Create a new API operation.
    pub fn new(summary: &str) -> Self {
        Self {
            summary: summary.into(),
            description: None,
            tags: Vec::new(),
            parameters: Vec::new(),
            responses: serde_json::json!({
                "200": { "description": "Success" }
            }),
        }
    }

    /// Set the operation description.
    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a tag.
    pub fn tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Add a path parameter.
    pub fn path_param(mut self, name: &str, schema_type: &str) -> Self {
        self.parameters.push(Parameter {
            name: name.into(),
            location: "path".into(),
            required: true,
            schema: ParameterSchema { schema_type: schema_type.into() },
            description: None,
        });
        self
    }

    /// Add a query parameter.
    pub fn query_param(mut self, name: &str, schema_type: &str) -> Self {
        self.parameters.push(Parameter {
            name: name.into(),
            location: "query".into(),
            required: false,
            schema: ParameterSchema { schema_type: schema_type.into() },
            description: None,
        });
        self
    }
}

/// API parameter definition.
#[derive(Debug, Clone, Serialize)]
pub struct Parameter {
    /// Parameter name.
    #[serde(rename = "name")]
    pub name: String,
    /// Parameter location: path, query, header, cookie.
    #[serde(rename = "in")]
    pub location: String,
    /// Whether the parameter is required.
    pub required: bool,
    /// Parameter schema.
    pub schema: ParameterSchema,
    /// Parameter description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Parameter schema for OpenAPI.
#[derive(Debug, Clone, Serialize)]
pub struct ParameterSchema {
    /// Schema data type.
    #[serde(rename = "type")]
    pub schema_type: String,
}

/// Security scheme definition.
#[derive(Debug, Clone, Serialize)]
pub struct SecurityScheme {
    /// Scheme type: http, apiKey, oauth2, openIdConnect.
    #[serde(rename = "type")]
    pub scheme_type: String,
    /// Scheme name for http type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    /// Bearer format for JWT.
    #[serde(rename = "bearerFormat", skip_serializing_if = "Option::is_none")]
    pub bearer_format: Option<String>,
}

impl SecurityScheme {
    /// Create a JWT bearer token security scheme.
    pub fn jwt() -> Self {
        Self {
            scheme_type: "http".into(),
            scheme: Some("bearer".into()),
            bearer_format: Some("JWT".into()),
        }
    }

    /// Create an API key security scheme.
    pub fn api_key(_location: &str) -> Self {
        Self {
            scheme_type: "apiKey".into(),
            scheme: None,
            bearer_format: None,
        }
    }
}
