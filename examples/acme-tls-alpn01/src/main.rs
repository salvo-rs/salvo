use salvo::prelude::*;

// This handler function responds with "Hello World" to any incoming request
#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    // Initialize the tracing subscriber for logging
    tracing_subscriber::fmt().init();

    // Create a new router and register the hello handler
    let router = Router::new().get(hello);

    // Set up a TCP listener on port 443 for HTTPS
    let acceptor = TcpListener::new("0.0.0.0:443")
        .acme() // Enable ACME for automatic SSL certificate management
        // .cache_path("temp/letsencrypt") // Specify the path to store the certificate cache (uncomment if needed)
        .add_domain("test.salvo.rs") // Replace this domain name with your own
        .bind()
        .await;

    // Start the server with the configured acceptor and router
    Server::new(acceptor).serve(router).await;
}
