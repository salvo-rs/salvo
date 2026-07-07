use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}
#[handler]
async fn hello_zh() -> Result<&'static str, ()> {
    Ok("你好，世界！")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    let router = Router::new()
        .get(hello)
        .push(Router::with_path("你好").get(hello_zh));
    println!("{router:?}");
    Server::new(acceptor)
        // This demo exercises the idle / write-stall / body timeouts, so opt into the full
        // set. `Server::new` already enables the safe handshake + header timeouts by default.
        .fuse_config(salvo::fuse::FuseConfig::strict())
        .serve(router)
        .await;
}
