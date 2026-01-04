use dotenvy::dotenv;
use sea_orm::DatabaseConnection;
use sea_orm::Database;
use sea_orm::DbErr;
use std::env;

pub type DbPool = DatabaseConnection;

pub async fn establish_connection_pool() -> DatabaseConnection {
    
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db: DatabaseConnection = Database::connect(database_url).await.unwrap();
    return db;
}

pub async fn check(db: DatabaseConnection) {
    // Should succeed
    assert!(db.ping().await.is_ok());

    // Close connection
    let cloned = db.clone();
    let _ = cloned.close().await;

    // Should now fail
    assert!(matches!(db.ping().await, Err(DbErr::ConnectionAcquire(_))));
}

