use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn execute(url: &str, table: &str, output_dir: &str) -> Result<()> {
    println!("Generating model for table '{}' from: {}", table, output_dir);

    let dir = Path::new(output_dir);
    fs::create_dir_all(dir)?;

    let model_name = to_pascal_case(table);
    let file_name = to_snake_case(table);

    let model_content = format!(
        r#"use sea_orm::{{entity::prelude::*, ActiveValue}};
use serde::{{Deserialize, Serialize}};

/// {} entity generated from table `{}`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "{}")]
pub struct Model {{
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Created at timestamp.
    pub created_at: DateTime,
    /// Updated at timestamp.
    pub updated_at: DateTime,
}}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {{}}

impl ActiveModelBehavior for ActiveModel {{}}

/// Domain type for {}.
#[derive(Debug, Clone)]
pub struct {} {{
    pub id: i64,
}}

impl From<Model> for {} {{
    fn from(m: Model) -> Self {{
        Self {{ id: m.id }}
    }}
}}
"#,
        model_name, table, table, model_name, model_name, model_name
    );

    fs::write(dir.join(format!("{}.rs", file_name)), model_content)?;

    // Generate mod.rs
    let mod_content = format!(
        r#"pub mod {};
pub use {}::Model as {}Model;
"#,
        file_name, file_name, model_name
    );

    fs::write(dir.join("mod.rs"), mod_content)?;

    // Generate repository.rs
    let repo_content = format!(
        r#"use sea_orm::{{DatabaseConnection, EntityTrait, QueryFilter, ColumnTrait, Set}};
use rszero::prelude::*;
use super::{};

/// Repository for {} entities.
pub struct {}Repository;

impl {}Repository {{
    pub async fn find_by_id(&self, db: &DatabaseConnection, id: i64) -> RszeroResult<Option<super::Model>> {{
        Entity::find_by_id(id).one(db).await.map_err(|e| RszeroError::Database(e.to_string()))
    }}

    pub async fn find_all(&self, db: &DatabaseConnection) -> RszeroResult<Vec<super::Model>> {{
        Entity::find().all(db).await.map_err(|e| RszeroError::Database(e.to_string()))
    }}

    pub async fn create(&self, db: &DatabaseConnection, id: i64) -> RszeroResult<super::Model> {{
        let active_model = ActiveModel {{
            id: Set(id),
            ..Default::default()
        }};
        active_model.insert(db).await.map_err(|e| RszeroError::Database(e.to_string()))
    }}

    pub async fn delete(&self, db: &DatabaseConnection, id: i64) -> RszeroResult<u64> {{
        let result = Entity::delete_by_id(id).exec(db).await.map_err(|e| RszeroError::Database(e.to_string()))?;
        Ok(result.rows_affected)
    }}
}}
"#,
        file_name, model_name, model_name, model_name
    );

    fs::write(dir.join(format!("{}_repository.rs", file_name)), repo_content)?;

    println!("  Generated model: {}/{}.rs", output_dir, file_name);
    println!("  Generated repository: {}/{}_repository.rs", output_dir, file_name);
    println!("  Generated mod.rs in {}/", output_dir);
    println!("Model code generation complete.");
    println!("\nNext steps:");
    println!("  1. Add `sea-orm = {{ version = \"0.12\", features = [\"sqlx-postgres\", \"runtime-tokio\"] }}` to dependencies");
    println!("  2. Use `sea-orm-cli generate entity -u \"{}\" -o {}` for full schema generation", url, output_dir);
    println!("  3. Or manually edit the generated model to match your table columns");
    Ok(())
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

fn to_snake_case(s: &str) -> String {
    s.to_lowercase()
}
