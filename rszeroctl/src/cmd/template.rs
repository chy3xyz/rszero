use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn execute(kind: &str, output_dir: &str) -> Result<()> {
    let dir = Path::new(output_dir);
    fs::create_dir_all(dir)?;

    let (template, ext) = match kind {
        "api" => (API_TEMPLATE, "api"),
        "rpc" => (RPC_TEMPLATE, "proto"),
        "model" => (MODEL_TEMPLATE, "rs"),
        _ => {
            println!("Unknown template kind: {}. Supported: api, rpc, model", kind);
            return Ok(());
        }
    };

    let out = dir.join(format!("example.{}", ext));
    fs::write(&out, template)?;
    println!("  Generated: {}", out.display());
    println!("Template file generated for kind: {}", kind);
    Ok(())
}

const API_TEMPLATE: &str = r#"// rszero API definition example
// Save as desc/user.api and run: rszeroctl api go --api desc/user.api --dir ./api

type GetUserReq {
    Id int64 `path:"id"`
}

type GetUserResp {
    Id   int64  `json:"id"`
    Name string `json:"name"`
    Age  int32  `json:"age"`
}

service user-api {
    @handler GetUser
    get /users/:id (GetUserReq) returns (GetUserResp)
}
"#;

const RPC_TEMPLATE: &str = r#"syntax = "proto3";
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

const MODEL_TEMPLATE: &str = r#"use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub age: i32,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
"#;
