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
    // only allow access from http://localhost:5800/, http://0.0.0.0:5800/ will get not found page.
    let router = Router::new()
        .filter_fn(|req, _| {
            // Extract HOST header from request
            let host = req.header::<String>("HOST").unwrap_or_default();
            // Only allow requests from localhost:5800
            host == "localhost:5800"
        })
        .get(hello);

    // Start server on port 5800
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
