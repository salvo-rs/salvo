use salvo::prelude::*;
use tokio::time::Duration;
use tower::limit::RateLimitLayer;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let limit = RateLimitLayer::new(5, Duration::from_secs(30)).compat();
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    let router = Router::new().hoop(limit).get(hello);
    Server::new(acceptor).serve(router).await;
}
