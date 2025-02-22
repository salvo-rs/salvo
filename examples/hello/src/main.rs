use salvo::prelude::*;

// Handler for English greeting
#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

// Handler for Chinese greeting
#[handler]
async fn hello_zh() -> Result<&'static str, ()> {
    Ok("你好，世界！")
}

#[tokio::main]
async fn main() {
    // Initialize logging subsystem
    tracing_subscriber::fmt().init();

    // Bind server to port 5800
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;

    // Create router with two endpoints:
    // - / (root path) returns English greeting
    // - /你好 returns Chinese greeting
    let router = Router::new()
        .get(hello)
        .push(Router::with_path("你好").get(hello_zh));

    // Print router structure for debugging
    println!("{:?}", router);

    // Start serving requests
    Server::new(acceptor).serve(router).await;
}
