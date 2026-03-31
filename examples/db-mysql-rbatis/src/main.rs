use std::sync::LazyLock;

use rbatis::RBatis;
use rbs::value;
use rbdc_mysql::driver::MysqlDriver;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

// Global RBatis instance for database operations
pub static RB: LazyLock<RBatis> = LazyLock::new(RBatis::new);

// User model representing the database table structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password: String,
}

// Generate CRUD methods for User model
rbatis::crud!(User{});

// Handler for retrieving a user by ID from the database
#[handler]
pub async fn get_user(req: &mut Request, res: &mut Response) {
    // Extract user ID from query parameters
    let uid = req.query::<i64>("uid").unwrap();
    // Execute select query and get user data
    let data: Vec<User> = User::select_by_map(&*RB, value!{"id": uid}).await.unwrap();
    let data = data.into_iter().next();
    println!("{data:?}");
    // Return user data as JSON response
    res.render(serde_json::to_string(&data).unwrap());
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Configure MySQL connection
    let mysql_uri = "mysql://root:123456@localhost/test";
    // Initialize RBatis with MySQL driver and connection URI
    RB.init(MysqlDriver {}, mysql_uri).unwrap();

    // Configure router with user endpoint:
    // - GET /users?uid={id} : Get user by ID
    let router = Router::with_path("users").get(get_user);

    // Start server on port 8698
    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(router).await;
}
