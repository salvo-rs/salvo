use salvo::prelude::*;

// Handler demonstrating streaming response using response channel
#[handler]
async fn hello(res: &mut Response) {
    // Set response content type to plain text
    res.add_header("content-type", "text/plain", true).unwrap();

    // Create a channel for streaming response data
    let mut tx = res.channel();

    // Spawn async task to send data through the channel
    tokio::spawn(async move {
        tx.send_data("Hello world").await.unwrap();
    });
}

#[tokio::main]
async fn main() {
    // Initialize logging subsystem
    tracing_subscriber::fmt().init();

    // Bind server to port 5800
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;

    // Create router with single endpoint
    let router = Router::new().get(hello);

    // Start serving requests
    Server::new(acceptor).serve(router).await;
}
