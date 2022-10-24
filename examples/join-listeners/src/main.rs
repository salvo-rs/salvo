use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello);
    let listener = TcpListener::bind("127.0.0.1:7878").join(TcpListener::bind("127.0.0.1:7979"));

    Server::new(listener).serve(router).await;
}
