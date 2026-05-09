use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn execute(api_file: &str, output_dir: &str) -> Result<()> {
    println!("Generating API code from: {}", api_file);

    // Parse the .api file using rszero's parser
    let api = rszero::api::parse_file(api_file)
        .map_err(|e| anyhow::anyhow!("failed to parse api file: {}", e))?;

    // Generate Rust code from the AST
    let generated = rszero::api::generate(&api)
        .map_err(|e| anyhow::anyhow!("code generation failed: {}", e))?;

    let dir = Path::new(output_dir);
    fs::create_dir_all(dir.join("handler"))?;
    fs::create_dir_all(dir.join("types"))?;
    fs::create_dir_all(dir.join("middleware"))?;

    // Write generated types
    fs::write(dir.join("types/mod.rs"), &generated.types)?;

    // Write generated handlers
    fs::write(dir.join("handler/mod.rs"), &generated.handlers)?;

    // Write generated routes (as a module for import)
    fs::write(dir.join("routes.rs"), &generated.routes)?;

    // Generate middleware module (lightweight logging middleware)
    let middleware_content = r#"use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Auto-generated request logging middleware.
pub async fn log_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let res = next.run(req).await;
    tracing::info!(%method, %uri, status = res.status().as_u16(), "request handled");
    res
}
"#;
    fs::write(dir.join("middleware/mod.rs"), middleware_content)?;

    // Generate main.rs that imports generated routes
    let routes_import = if !generated.routes.is_empty() {
        "mod routes;\n"
    } else {
        ""
    };

    let main_content = format!(r#"use rszero::prelude::*;
use axum::routing::{{get, post, put, delete}};

mod handler;
mod types;
mod middleware;
{routes_import}

#[tokio::main]
async fn main() -> anyhow::Result<()> {{
    let config = load_config("etc/api.yaml")?;
    log::init(&config.log);

    let mut server = RszeroServer::from_config(&config);
    // Register auto-generated routes
    server = routes::register_routes(server);

    server.start().await?;
    Ok(())
}}
"#);

    fs::write(dir.join("main.rs"), main_content)?;

    println!("  Generated types in {}/types/", output_dir);
    println!("  Generated handlers in {}/handler/", output_dir);
    println!("  Generated routes in {}/routes.rs", output_dir);
    println!("  Generated main.rs in {}/main.rs", output_dir);
    println!("API code generation complete.");
    Ok(())
}
