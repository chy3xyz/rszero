use anyhow::Result;
use std::fs;
use std::path::Path;


pub fn execute(idl_file: &str, output_dir: &str) -> Result<()> {
    println!("Generating RPC code from: {}", idl_file);

    let dir = Path::new(output_dir);
    fs::create_dir_all(dir.join("logic"))?;
    fs::create_dir_all(dir.join("svc"))?;
    fs::create_dir_all(dir.join("model"))?;

    // Generate proto stub if the IDL file doesn't exist
    let idl_path = Path::new(idl_file);
    if !idl_path.exists() {
        println!("  IDL file not found, generating proto stub...");
        let proto_content = r#"syntax = "proto3";
package user;
option go_package = "./user";

message GetUserReq {
    int64 id = 1;
}

message GetUserResp {
    int64 id = 1;
    string name = 2;
    int32 age = 3;
}

service UserService {
    rpc GetUser(GetUserReq) returns (GetUserResp);
}
"#;
        fs::write(idl_path, proto_content)?;
    }

    // Generate Rust service logic stub
    let logic_content = r#"use rszero::prelude::*;

/// Business logic for the generated RPC service.
pub struct ServiceLogic;

impl ServiceLogic {
    pub async fn get_user(&self, id: i64) -> RszeroResult<User> {
        Ok(User {
            id,
            name: "demo".into(),
            age: 25,
        })
    }
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub age: i32,
}
"#;

    fs::write(dir.join("logic/mod.rs"), logic_content)?;

    // Generate service wrapper stub
    let svc_content = r#"use rszero::prelude::*;

/// Generated service wrapper — integrate with volo-build output.
pub struct Svc;

impl Svc {
    pub fn new() -> Self {
        Self
    }
}
"#;

    fs::write(dir.join("svc/mod.rs"), svc_content)?;

    // Generate main.rs for the RPC service
    let main_content = r#"use rszero::prelude::*;

mod logic;
mod svc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = load_config("../../etc/rpc.yaml")?;
    log::init(&config.log);

    let server = RpcServer::from_config(&config)
        .service_name("user-rpc");

    server.start().await?;
    Ok(())
}
"#.to_string();

    fs::write(dir.join("main.rs"), main_content)?;

    // Generate build.rs for volo-build
    let build_rs = r#"fn main() {
    // Uncomment after adding volo-build to dependencies:
    // volo_build::Builder::protobuf()
    //     .add_service("idl/user.proto")
    //     .filename("user.rs")
    //     .write()
    //     .unwrap();
}
"#;
    fs::write(dir.join("build.rs"), build_rs)?;

    println!("  Generated logic stubs in {}/logic/", output_dir);
    println!("  Generated svc stubs in {}/svc/", output_dir);
    println!("  Generated main.rs in {}/main.rs", output_dir);
    println!("RPC code generation complete.");
    println!("\nNext steps:");
    println!("  1. Add volo-build to [build-dependencies] in {}/Cargo.toml", output_dir);
    println!("  2. Run `cargo build` to generate service stubs");
    Ok(())
}
