use anyhow::Result;
use std::fs;
use std::path::Path;

fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    println!("  write: {}", path.display());
    Ok(())
}

pub fn execute(name: &str) -> Result<()> {
    println!("Creating rszero project: {}", name);

    let base = Path::new(name);

    write_file(&base.join("Cargo.toml"), &root_cargo_toml(name))?;
    write_file(&base.join(".env"), ENV_TEMPLATE)?;
    write_file(&base.join("etc/api.yaml"), API_CONFIG_TEMPLATE)?;
    write_file(&base.join("etc/user-rpc.yaml"), RPC_CONFIG_TEMPLATE)?;
    write_file(&base.join("api/desc/.gitkeep"), "")?;
    write_file(&base.join("api/main.rs"), API_MAIN_TEMPLATE)?;
    write_file(&base.join("rpc/user/desc/.gitkeep"), "")?;
    write_file(&base.join("rpc/user/logic/.gitkeep"), "")?;
    write_file(&base.join("rpc/user/svc/.gitkeep"), "")?;
    write_file(&base.join("rpc/user/model/.gitkeep"), "")?;
    write_file(&base.join("rpc/user/main.rs"), RPC_MAIN_TEMPLATE)?;
    write_file(&base.join("rpc/user/Cargo.toml"), &rpc_cargo_toml(name))?;
    write_file(&base.join("idl/.gitkeep"), "")?;
    write_file(&base.join("common/.gitkeep"), "")?;
    write_file(&base.join("deploy/.gitkeep"), "")?;

    println!("\nProject '{}' created successfully!", name);
    println!("  cd {}", name);
    println!("  cargo build");
    Ok(())
}

fn root_cargo_toml(name: &str) -> String {
    format!(
        r#"[workspace]
members = [
    "api",
    "rpc/user",
    "common",
]
resolver = "2"

[workspace.package]
name = "{name}"
edition = "2021"
version = "0.1.0"

[workspace.dependencies]
rszero = "0.1"
tokio = {{ version = "1.0", features = ["full"] }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
"#
    )
}

fn rpc_cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}-user-rpc"
edition.workspace = true
version.workspace = true

[dependencies]
rszero.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
"#
    )
}

const ENV_TEMPLATE: &str = "RSZERO_LOG_LEVEL=debug\nRSZERO_CACHE_HOST=127.0.0.1\nRSZERO_CACHE_PORT=6379\n";

const API_CONFIG_TEMPLATE: &str = "Name: user-api\nHost: 0.0.0.0\nPort: 8080\nLog:\n  Level: info\n  Format: json\nCache:\n  Host: 127.0.0.1\n  Port: 6379\n";

const RPC_CONFIG_TEMPLATE: &str = "Name: user-rpc\nListenOn: 0.0.0.0:8081\nEtcd:\n  Hosts:\n    - 127.0.0.1:2379\n  Key: user.rpc\nLog:\n  Level: info\n  Format: json\n";

const API_MAIN_TEMPLATE: &str = r#"use rszero::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = load_config("etc/api.yaml")?;
    log::init(&config.log);

    let server = RszeroServer::from_config(&config);
    server.start().await?;

    Ok(())
}
"#;

const RPC_MAIN_TEMPLATE: &str = r#"use rszero::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = load_config("../../etc/user-rpc.yaml")?;
    log::init(&config.log);

    let rpc_server = RpcServer::from_config(&config);
    rpc_server.start().await?;

    Ok(())
}
"#;
