use sea_orm_migration::prelude::*;

// Migration struct for creating posts table
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // Up migration: Creates the posts table
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Posts::Table)
                    .if_not_exists()
                    // Define id column as auto-incrementing primary key
                    .col(
                        ColumnDef::new(Posts::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    // Define title column as non-null string
                    .col(ColumnDef::new(Posts::Title).string().not_null())
                    // Define text column as non-null string
                    .col(ColumnDef::new(Posts::Text).string().not_null())
                    .to_owned(),
            )
            .await
    }

    // Down migration: Drops the posts table
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Posts::Table).to_owned())
            .await
    }
}

// Enum representing table and column names
// Used for type-safe table/column name references
#[derive(Iden)]
enum Posts {
    Table, // Represents the table name
    Id,    // Represents the id column
    Title, // Represents the title column
    Text,  // Represents the text column
}
