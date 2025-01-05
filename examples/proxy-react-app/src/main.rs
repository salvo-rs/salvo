use salvo::prelude::*;
use salvo::proxy::HyperClient;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("{**rest}").goal(Proxy::new(
        vec!["http://localhost:3000"],
        HyperClient::default(),
    ));
    println!("{:?}", router);

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
