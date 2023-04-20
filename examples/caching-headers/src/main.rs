use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // CachingHeader must be before Compression.
    let router = Router::with_hoop(CachingHeaders::new())
        .hoop(Compression::new().with_min_length(0))
        .get(hello);
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
