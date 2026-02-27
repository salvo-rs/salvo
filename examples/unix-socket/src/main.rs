#[cfg(all(feature = "unix", unix))]
#[tokio::main]
async fn main() {
    use salvo::prelude::*;

    let router = Router::with_path("files/{*path}").get(StaticDir::new("./static"));
    let acceptor = UnixListener::new("/tmp/salvo.sock").bind().await;
    Server::new(acceptor).serve(router).await;
}

#[cfg(not(all(feature = "unix", unix)))]
#[tokio::main]
async fn main() {
    println!("This example requires the 'unix' feature and must be run on a Unix system (Linux, macOS, BSD)");
}
