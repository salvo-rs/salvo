use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // Compression must be before CachingHeader.
    let router = Router::with_hoop(CachingHeaders::new())
        .hoop(Compression::new().with_min_length(0))
        .get(hello);
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
