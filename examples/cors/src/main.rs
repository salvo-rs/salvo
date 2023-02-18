use salvo::cors::Cors;
use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "hello"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let cors_handler = Cors::builder()
        .allow_origin("https://salvo.rs")
        .allow_methods(vec!["GET", "POST", "DELETE"])
        .build();

    let router = Router::with_hoop(cors_handler).get(hello).options(handler::empty());

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
