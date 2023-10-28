use salvo::prelude::*;

#[handler]
async fn hello(res: &mut Response) {
    res.add_header("content-type", "text/plain", true).unwrap();
    let mut tx = res.channel();
    tokio::spawn(async move {
        tx.send_data("Hello world").await.unwrap();
    });
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    let router = Router::new().get(hello);
    Server::new(acceptor).serve(router).await;
}
