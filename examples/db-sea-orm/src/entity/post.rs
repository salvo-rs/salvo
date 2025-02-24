use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

// Blog post model representing the database table structure
// Implements various traits for ORM functionality and serialization
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    // Primary key field with auto-increment
    #[sea_orm(primary_key)]
    #[serde(skip_deserializing)]
    pub id: i32,

    // Post title field
    pub title: String,

    // Post content field using Text type for longer content
    #[sea_orm(column_type = "Text")]
    pub text: String,
}

// Define possible relations with other entities
// Currently empty as this is a standalone model
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

// Implement default active model behavior
// This enables standard CRUD operations
impl ActiveModelBehavior for ActiveModel {}
