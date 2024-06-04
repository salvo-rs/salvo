use salvo::prelude::*;
use salvo::trailing_slash::add_slash;

#[handler]
async fn hello() -> &'static str {
    "Hello"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let router = Router::with_hoop(add_slash())
        .push(Router::with_path("hello").get(hello))
        .push(Router::with_path("hello.world").get(hello));
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
