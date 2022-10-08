use salvo::prelude::*;
use salvo::proxy::Proxy;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<**rest>").handle(Proxy::new(vec!["http://localhost:7878"]));
    println!("{:?}", router);
    tracing::info!("Listening on http://127.0.0.1:8888");
    tracing::info!("Run `cargo run --bin example-ws-chat` to start websocket chat server");
    Server::new(TcpListener::bind("127.0.0.1:8888")).serve(router).await;
}
