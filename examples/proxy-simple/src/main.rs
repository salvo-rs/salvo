use salvo::prelude::*;
use salvo::proxy::Proxy;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(
            Router::new()
                .host("127.0.0.1")
                .path("<**rest>")
                .goal(Proxy::default_hyper_client("https://www.rust-lang.org")),
        )
        .push(
            Router::new()
                .host("localhost")
                .path("<**rest>")
                .goal(Proxy::default_hyper_client("https://crates.io")),
        );

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
