//! Code generator: `.api` AST → Rust code.
//!
//! Generates handler stubs, request/response types, and route registration
//! from a parsed `.api` file.

use super::parser::{ApiFile, ApiType, ApiRoute, RouteMethod};
use crate::error::RszeroResult;

/// Generated Rust code bundle.
#[derive(Debug, Clone)]
pub struct GeneratedCode {
    /// Generated type definitions.
    pub types: String,
    /// Generated handler stubs.
    pub handlers: String,
    /// Generated route registration.
    pub routes: String,
}

/// Generate Rust code from a parsed `.api` file.
pub fn generate(api: &ApiFile) -> RszeroResult<GeneratedCode> {
    let mut types_code = String::new();
    types_code.push_str("// Auto-generated from .api file\n\n");
    types_code.push_str("use serde::{Serialize, Deserialize};\n\n");

    for ty in &api.types {
        types_code.push_str(&generate_type(ty));
        types_code.push('\n');
    }

    let mut handlers_code = String::new();
    handlers_code.push_str("// Auto-generated handler stubs\n\n");
    handlers_code.push_str("use axum::response::IntoResponse;\n");
    handlers_code.push_str("use rszero::prelude::*;\n\n");

    let mut routes_code = String::new();
    routes_code.push_str("// Auto-generated route registration\n");
    routes_code.push_str("pub fn register_routes(server: RszeroServer) -> RszeroServer {\n");
    routes_code.push_str("    server\n");

    for service in &api.services {
        for route in &service.routes {
            handlers_code.push_str(&generate_handler(route));
            handlers_code.push('\n');
            routes_code.push_str(&generate_route_registration(route));
        }
    }

    routes_code.push_str("}\n");

    Ok(GeneratedCode {
        types: types_code,
        handlers: handlers_code,
        routes: routes_code,
    })
}

fn generate_type(ty: &ApiType) -> String {
    let mut code = "#[derive(Debug, Clone, Serialize, Deserialize)]\n".to_string();
    code.push_str(&format!("pub struct {} {{\n", ty.name));
    for field in &ty.fields {
        let tag = field.tag.as_ref()
            .map(|t| format!("    #[serde({})]\n", format_tag(t)))
            .unwrap_or_default();
        let rust_type = map_api_type(&field.ty);
        code.push_str(&tag);
        code.push_str(&format!("    pub {}: {},\n", snake_case(&field.name), rust_type));
    }
    code.push_str("}\n");
    code
}

fn generate_handler(route: &ApiRoute) -> String {
    let handler_name = &route.handler;
    let req_type = route.request_type.as_deref().unwrap_or("()");
    let resp_type = route.response_type.as_deref().unwrap_or("impl IntoResponse");

    let mut code = format!("/// Handler: {}\n", handler_name);
    code.push_str(&format!(
        "pub async fn {}(_req: {}) -> {} {{\n",
        handler_name, req_type, resp_type
    ));
    code.push_str("    // TODO: implement business logic\n");
    code.push_str("    ok(\"ok\")\n");
    code.push_str("}\n");
    code
}

fn generate_route_registration(route: &ApiRoute) -> String {
    let _method = route.method.as_str().to_lowercase();
    let handler = &route.handler;
    let path = &route.path;

    let axum_method = match route.method {
        RouteMethod::Get => "get",
        RouteMethod::Post => "post",
        RouteMethod::Put => "put",
        RouteMethod::Delete => "delete",
        RouteMethod::Patch => "patch",
        RouteMethod::Head => "head",
    };

    format!(
        "        .route(\"{}\", axum::routing::{}({}))\n",
        path, axum_method, handler
    )
}

fn map_api_type(api_type: &str) -> String {
    match api_type {
        "string" | "String" => "String".into(),
        "int" | "int32" => "i32".into(),
        "int64" => "i64".into(),
        "bool" => "bool".into(),
        "float" | "float32" => "f32".into(),
        "float64" => "f64".into(),
        t if t.starts_with('[') && t.ends_with(']') => {
            let inner = &t[1..t.len()-1];
            format!("Vec<{}>", map_api_type(inner))
        }
        t if t.starts_with("map[") => "std::collections::HashMap<String, String>".into(),
        other => other.into(),
    }
}

fn format_tag(tag: &str) -> String {
    // Input: json:"id"
    // Output: rename = "id"
    if let Some(pos) = tag.find(':') {
        let key = &tag[..pos];
        let value = tag[pos+1..].trim_matches('"');
        if key == "json" {
            format!("rename = \"{}\"", value)
        } else {
            format!("{} = \"{}\"", key, value)
        }
    } else {
        tag.to_string()
    }
}

fn snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().next().expect("every char has at least one lowercase mapping"));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::parser::parse_api;

    #[test]
    fn test_generate_types() {
        let source = r#"
type User {
    Id   int64  `json:"id"`
    Name string `json:"name"`
}
"#;
        let api = parse_api(source).unwrap();
        let gen = generate(&api).unwrap();
        assert!(gen.types.contains("struct User"));
        assert!(gen.types.contains("pub id: i64"));
        assert!(gen.types.contains("pub name: String"));
    }

    #[test]
    fn test_generate_handlers() {
        let source = r#"
service user-api {
    @handler getUser
    get /users/:id (GetUserReq) returns (User)
}
"#;
        let api = parse_api(source).unwrap();
        let gen = generate(&api).unwrap();
        assert!(gen.handlers.contains("pub async fn getUser"));
        assert!(gen.routes.contains(".route(\"/users/:id\""));
    }

    #[test]
    fn test_map_api_type() {
        assert_eq!(map_api_type("string"), "String");
        assert_eq!(map_api_type("int64"), "i64");
        assert_eq!(map_api_type("[string]"), "Vec<String>");
    }

    #[test]
    fn test_snake_case() {
        assert_eq!(snake_case("UserId"), "user_id");
        assert_eq!(snake_case("name"), "name");
    }
}
