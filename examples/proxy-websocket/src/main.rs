use salvo::prelude::*;
use salvo::proxy::HyperClient;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("{**rest}").goal(Proxy::new(
        vec!["http://localhost:5800"],
        HyperClient::default(),
    ));
    println!("{:?}", router);
    tracing::info!("Run `cargo run --bin example-websocket-chat` to start websocket chat server");
    let acceptor = TcpListener::new("0.0.0.0:8888").bind().await;
    Server::new(acceptor).serve(router).await;
}
