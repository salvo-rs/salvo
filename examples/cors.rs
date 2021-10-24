use salvo::prelude::*;
use salvo_extra::cors::CorsHandler;

#[fn_handler]
async fn hello() -> &'static str {
    "hello"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let cors_handler = CorsHandler::builder()
        .with_allow_origin("https://salvo.rs")
        .with_allow_methods(vec!["GET", "POST", "DELETE"])
        .build();

    let router = Router::with_before(cors_handler).get(hello);
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
