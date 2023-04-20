use salvo::prelude::*;
use salvo::proxy::Proxy;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(Router::with_path("google/<**rest>").handle(Proxy::new(["https://www.google.com"])))
        .push(Router::with_path("rust/<**rest>").handle(Proxy::new("https://www.rust-lang.org")));

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
