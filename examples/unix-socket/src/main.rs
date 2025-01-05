#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() {
    use salvo::prelude::*;

    let router = Router::with_path("files/{*path}").get(StaticDir::new("./static"));
    let acceptor = UnixListener::new("/tmp/salvo.sock").bind().await;
    Server::new(acceptor).serve(router).await;
}

#[cfg(not(target_os = "linux"))]
#[tokio::main]
async fn main() {
    println!("please run this example in linux");
}
