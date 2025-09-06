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

    // Configure router with custom host filter
    // only allow access from http://localhost:8698/, http://0.0.0.0:8698/ will get not found page.
    let router = Router::new()
        .filter_fn(|req, _| {
            // Extract HOST header from request
            let host = req.header::<String>("HOST").unwrap_or_default();
            // Only allow requests from localhost:8698
            host == "localhost:8698"
        })
        .get(hello);

    // Start server on port 8698
    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(router).await;
}
