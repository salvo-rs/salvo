use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello);
    let acceptor = TcpListener::new("0.0.0.0:5800")
        .join(TcpListener::new("0.0.0.0:5801"))
        .bind()
        .await;

    Server::new(acceptor).serve(router).await;
}
