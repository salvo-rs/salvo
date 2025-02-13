use salvo::prelude::*;

// Handler that deliberately panics to demonstrate panic catching
#[handler]
async fn hello() {
    panic!("panic error!");
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Set up router with CatchPanic middleware to handle panics gracefully
    // This prevents the server from crashing when a panic occurs in a handler
    let router = Router::new().hoop(CatchPanic::new()).get(hello);

    // Bind server to port 5800 and start serving
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
