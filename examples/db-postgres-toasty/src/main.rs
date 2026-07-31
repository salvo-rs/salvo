pub mod common;
mod sys_user_handler;
mod sys_user_model;
pub mod sys_user_service;
pub mod sys_user_vo;

use crate::sys_user_handler::{
    add_sys_user, delete_sys_user, query_sys_user_detail, query_sys_user_list, update_sys_user,
    update_sys_user_status,
};
use salvo::prelude::*;
use tracing::warn;
use tracing_appender::rolling;
use tracing_subscriber::{Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize logging system
    let file_appender = rolling::daily("./db-postgres-toasty/logs", "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // config console layer
    let console_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_filter(tracing_subscriber::filter::LevelFilter::DEBUG);
    // config file layer
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_filter(tracing_subscriber::filter::LevelFilter::DEBUG);

    tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .init();

    // Configure PostgreSQL connection
    let postgres_uri = "postgres://postgres:password@localhost/test";
    // Create and initialize connection pool
    let db = init_db(postgres_uri).await;

    let s = db.push_schema().await;
    if s.is_err() {
        warn!("{}", s.err().unwrap());
    }
    // Store db in global state and Configure router with user
    let router = Router::new()
        .hoop(affix_state::insert("db", db))
        .push(build_sys_user_route());

    // Start server on port 8698
    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(router).await;
}


/// init db
pub async fn init_db(url: &str) -> toasty::Db {
    toasty::Db::builder()
        .models(toasty::models!(crate::*))
        .slow_statement_threshold(Some(std::time::Duration::from_millis(250)))
        .log_statement_params(true)
        .connect(url)
        .await
        .expect("Failed to connect to database")
}

/// build sys user route
pub fn build_sys_user_route() -> Router {
    Router::new()
        .push(Router::new().path("/api/user/addUser").post(add_sys_user))
        .push(
            Router::new()
                .path("/api/user/deleteUser")
                .post(delete_sys_user),
        )
        .push(
            Router::new()
                .path("/api/user/updateUser")
                .post(update_sys_user),
        )
        .push(
            Router::new()
                .path("/api/user/updateUserStatus")
                .post(update_sys_user_status),
        )
        .push(
            Router::new()
                .path("/api/user/queryUserDetail")
                .post(query_sys_user_detail),
        )
        .push(
            Router::new()
                .path("/api/user/queryUserList")
                .post(query_sys_user_list),
        )
}
