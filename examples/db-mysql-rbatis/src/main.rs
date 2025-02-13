#[macro_use]
extern crate rbatis;
extern crate rbdc;

use std::sync::LazyLock;

use rbatis::RBatis;
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

// Implement select query for User model
// Generates SQL: SELECT * FROM user WHERE id = #{id} LIMIT 1
impl_select!(User{select_by_id(id:String) -> Option => "`where id = #{id} limit 1`"});

// Handler for retrieving a user by ID from the database
#[handler]
pub async fn get_user(req: &mut Request, res: &mut Response) {
    // Extract user ID from query parameters
    let uid = req.query::<i64>("uid").unwrap();
    // Execute select query and get user data
    let data = User::select_by_id(&*RB, uid.to_string()).await.unwrap();
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

    // Start server on port 5800
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
