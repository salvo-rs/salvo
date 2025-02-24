use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;

// Handler function responding with "Hello World" for HTTP/3 requests
#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Load TLS certificate and private key from embedded PEM files
    let cert = include_bytes!("../certs/cert.pem").to_vec();
    let key = include_bytes!("../certs/key.pem").to_vec();

    // Create router with single endpoint
    let router = Router::new().get(hello);

    // Configure TLS settings using Rustls
    let config = RustlsConfig::new(Keycert::new().cert(cert.as_slice()).key(key.as_slice()));

    // Create TCP listener with TLS encryption on port 5800
    let listener = TcpListener::new(("0.0.0.0", 5800)).rustls(config.clone());

    // Create QUIC listener and combine with TCP listener
    let acceptor = QuinnListener::new(config.build_quinn_config().unwrap(), ("0.0.0.0", 5800))
        .join(listener)
        .bind()
        .await;

    // Start server supporting both HTTP/3 (QUIC) and HTTPS (TCP)
    Server::new(acceptor).serve(router).await;
}
