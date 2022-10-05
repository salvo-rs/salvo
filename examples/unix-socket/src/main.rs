#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() {
    use salvo::prelude::*;
    use salvo_static::StaticDir;

    let router = Router::with_path("files/<*path>").get(StaticDir::new("./static"));
    Server::new(UnixListener::bind("/tmp/salvo.sock")).serve(router).await;
}

#[cfg(not(target_os = "linux"))]
#[tokio::main]
async fn main() {
    println!("please run this example in linux");
}
