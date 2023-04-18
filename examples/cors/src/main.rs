use salvo::cors::Cors;
use salvo::http::Method;
use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "hello"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let cors_handler = Cors::new()
        .allow_origin("https://salvo.rs")
        .allow_methods(vec![Method::GET, Method::POST, Method::DELETE])
        .into_handler();

    let router = Router::with_hoop(cors_handler).get(hello).options(handler::empty());

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
