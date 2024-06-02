use std::time::Duration;

use salvo::prelude::*;

#[handler]
async fn fast() -> &'static str {
    "hello"
}
#[handler]
async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_secs(6)).await;
    "hello"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;

    let router = Router::new()
        .hoop(Timeout::new(Duration::from_secs(5)))
        .push(Router::with_path("slow").get(slow))
        .push(Router::with_path("fast").get(fast));

    Server::new(acceptor).serve(router).await;
}
