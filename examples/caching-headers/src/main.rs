use salvo::prelude::*;

// Handler that returns a simple "Hello World" response
#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Set up router with caching headers and compression middleware
    // CachingHeader must be before Compression to properly set cache control headers
    let router = Router::with_hoop(CachingHeaders::new())
        .hoop(Compression::new().min_length(0)) // Enable compression for all responses
        .get(hello);

    // Bind server to port 5800 and start serving
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
