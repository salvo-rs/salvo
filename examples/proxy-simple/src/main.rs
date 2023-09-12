use salvo::prelude::*;
use salvo::proxy::Proxy;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(
            Router::new()
                .host("0.0.0.0")
                .path("<**rest>")
                .goal(Proxy::new("https://www.rust-lang.org")),
        )
        .push(
            Router::new()
                .host("localhost")
                .path("<**rest>")
                .goal(Proxy::new("https://crates.io")),
        );

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
