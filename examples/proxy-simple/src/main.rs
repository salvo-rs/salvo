use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // In this example, if the requested URL begins with <http://127.0.0.1:5800/>, the proxy goes to
    // <https://www.rust-lang.org>; if the requested URL begins with <http://localhost:5800/>, the proxy
    // goes to <https://crates.io>.
    let router = Router::new()
        .push(
            Router::new()
                .host("127.0.0.1")
                .path("{**rest}")
                .goal(Proxy::use_hyper_client("https://www.rust-lang.org")),
        )
        .push(
            Router::new()
                .host("localhost")
                .path("{**rest}")
                .goal(Proxy::use_hyper_client("https://crates.io")),
        );

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
