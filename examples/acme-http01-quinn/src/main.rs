use salvo::prelude::*;

// Handler function that returns "Hello World" for any request
#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt().init();

    // Create a router and register the hello handler
    let mut router = Router::new().get(hello);

    // Set up a TCP listener on port 443 for HTTPS
    let listener = TcpListener::new("0.0.0.0:443")
        .acme() // Enable ACME for automatic SSL certificate management
        .cache_path("temp/letsencrypt") // Path to store the certificate cache
        .add_domain("test.salvo.rs") // replace with your domain
        .http01_challenge(&mut router) // Add routes to handle ACME challenge requests
        .quinn("0.0.0.0:443"); // Enable QUIC/HTTP3 support on the same port

    // Create an acceptor that listens on both port 80 (HTTP) and port 443 (HTTPS)
    let acceptor = listener.join(TcpListener::new("0.0.0.0:80")).bind().await;

    // Start the server with the configured acceptor and router
    Server::new(acceptor).serve(router).await;
}
