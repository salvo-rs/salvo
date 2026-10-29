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
    let router = Router::new().get(hello);
    let mut challenge_router = Router::new();

    // Set up a TCP listener on port 443 for HTTPS
    let listener = TcpListener::new("0.0.0.0:443")
        .acme() // Enable ACME for automatic SSL certificate management
        .cache_path("temp/letsencrypt") // Path to store the certificate cache
        .add_domain("test.salvo.rs") // replace with your domain
        .http01_challenge(&mut challenge_router)
        .quinn("0.0.0.0:443"); // Enable QUIC/HTTP/3 support on the same port

    // Serve challenges before TLS binding waits for initial CA validation.
    let http = TcpListener::new("0.0.0.0:80").bind().await;
    let http_task = tokio::spawn(Server::new(http).serve(challenge_router));
    let acceptor = listener.bind().await;

    // Start the server with the configured acceptor and router
    Server::new(acceptor).serve(router).await;
    http_task.abort();
}
