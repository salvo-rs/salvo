use salvo::prelude::*;
use salvo::cors::Cors;

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

    let router = Router::with_hoop(cors_handler).get(hello).options(EmptyHandler);
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
