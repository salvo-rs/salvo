use salvo::prelude::*;

#[handler]
async fn hello(depot: &mut Depot) -> String {
    format!("Request id: {:?}", depot.request_id())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    let router = Router::new().hoop(RequestId::new()).get(hello);
    Server::new(acceptor).serve(router).await;
}