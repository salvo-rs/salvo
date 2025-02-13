use sea_orm_migration::prelude::*;

// Entry point for running database migrations via CLI
#[async_std::main]
async fn main() {
    // Run the migration CLI with our Migrator
    cli::run_cli(migration::Migrator).await;
}
