use std::sync::Arc;

use salvo::cors::{AllowOrigin, Cors};
use salvo::http::Method;
use salvo::oapi::extract::QueryParam;
use salvo::oapi::{OpenApi, endpoint};
use salvo::prelude::*;

use crate::db::{DbPool, establish_connection_pool};
use crate::routers::posts::get_posts_router;
use crate::routers::users::get_users_router;

pub mod auth;
pub mod db;
pub mod models;
pub mod routers;
pub mod schemas;
pub mod tests;
pub mod utils;

#[endpoint(
    tags("Main"),
    summary = "hello",
    description = "description  of the  main endpoint"
)]
pub async fn hello(name: QueryParam<String, false>, res: &mut Response, depot: &mut Depot) {
    println!("{:?}", name);
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut _conn = pool.get().expect("Failed to get DB connection");
    res.status_code(StatusCode::OK);
    res.render(format!("Hello, {}!", name.clone().unwrap()))
}

#[endpoint(
    tags("Hello"),
    summary = "Just Print hello world",
    description = "description of the handle/endpoint to print hello world"
)]
pub async fn hello_world(res: &mut Response) -> Result<&'static str, salvo::Error> {
    res.status_code(StatusCode::OK);
    Ok("Hello world")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let pool = Arc::new(establish_connection_pool());

    let cors = Cors::new()
        .allow_origin(AllowOrigin::any())
        .allow_methods(vec![Method::GET, Method::POST, Method::DELETE])
        .allow_headers("authorization")
        .allow_headers("authentication")
        .into_handler();

    let router = Router::new()
        .hoop(affix_state::inject(pool))
        .get(hello_world)
        .push(Router::with_path("hello").get(hello))
        .push(get_users_router())
        .push(get_posts_router());

    let doc = OpenApi::new("Salvo Postgresql Boilerplate", "0.0.1").merge_router(&router);

    let router = router
        .unshift(doc.into_router("/api-doc/openapi.json"))
        .unshift(SwaggerUi::new("/api-doc/openapi.json").into_router("/docs"));

    let service = Service::new(router).hoop(cors);

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;

    Server::new(acceptor).serve(service).await;
}
