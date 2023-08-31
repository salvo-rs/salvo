use salvo::prelude::*;

#[handler]
async fn hello() {
    panic!("panic error!");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().hoop(CatchPanic::new()).get(hello);

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
