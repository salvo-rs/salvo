#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() {
    use salvo::extra::serve::StaticDir;
    use salvo::prelude::*;

    let router = Router::with_path("files/<*path>").get(StaticDir::new("./static"));
    Server::builder(UnixListener::bind("/tmp/salvo.sock").unwrap())
        .serve(Service::new(router))
        .await
        .unwrap();
}

#[cfg(not(target_os = "linux"))]
#[tokio::main]
async fn main() {
    println!("please run this example in linux");
}
