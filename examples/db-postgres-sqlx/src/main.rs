use std::sync::OnceLock;

use salvo::prelude::*;
use serde::Serialize;
use sqlx::{FromRow, PgPool};

// Global PostgreSQL connection pool instance
static POSTGRES: OnceLock<PgPool> = OnceLock::new();

// Helper function to get the PostgreSQL connection pool
#[inline]
pub fn get_postgres() -> &'static PgPool {
    POSTGRES.get().unwrap()
}

// User model representing the database table structure
// Implements FromRow for SQL query results and Serialize for JSON responses
#[derive(FromRow, Serialize, Debug)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password: String,
}

// Handler for retrieving a user by ID from the database
#[handler]
pub async fn get_user(req: &mut Request, res: &mut Response) {
    // Extract user ID from query parameters
    let uid = req.query::<i64>("uid").unwrap();
    // Execute SQL query to fetch user by ID
    let data = sqlx::query_as::<_, User>("select * from users where id = $1")
        .bind(uid)
        .fetch_one(get_postgres())
        .await
        .unwrap();
    // Return user data as JSON response
    res.render(serde_json::to_string(&data).unwrap());
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Configure PostgreSQL connection
    let postgres_uri = "postgres://postgres:password@localhost/test";
    // Create and initialize connection pool
    let pool = PgPool::connect(postgres_uri).await.unwrap();
    // Store pool in global state
    POSTGRES.set(pool).unwrap();

    // Configure router with user endpoint:
    // - GET /users?uid={id} : Get user by ID
    let router = Router::with_path("users").get(get_user);

    // Start server on port 5800
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
