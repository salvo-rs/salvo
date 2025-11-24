pub use sea_orm_migration::prelude::*;

// Import the migration for creating posts table
mod m20220120_000001_create_post_table;

// Main migrator struct that manages all migrations
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    // Returns a list of all migrations in order
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20220120_000001_create_post_table::Migration)]
    }
}
