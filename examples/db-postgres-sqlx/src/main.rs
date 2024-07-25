use std::sync::OnceLock;

use salvo::prelude::*;
use serde::Serialize;
use sqlx::{FromRow, PgPool};

static POSTGRES: OnceLock<PgPool> = OnceLock::new();

#[inline]
pub fn get_postgres() -> &'static PgPool {
    POSTGRES.get().unwrap()
}

#[derive(FromRow, Serialize, Debug)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password: String,
}

#[handler]
pub async fn get_user(req: &mut Request, res: &mut Response) {
    let uid = req.query::<i64>("uid").unwrap();
    let data = sqlx::query_as::<_, User>("select * from users where id = $1")
        .bind(uid)
        .fetch_one(get_postgres())
        .await
        .unwrap();
    res.render(serde_json::to_string(&data).unwrap());
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // postgresql connect info
    let postgres_uri = "postgres://postgres:password@localhost/test";
    let pool = PgPool::connect(postgres_uri).await.unwrap();
    POSTGRES.set(pool).unwrap();

    // router
    let router = Router::with_path("users").get(get_user);

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
