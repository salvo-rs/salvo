//port from https://github.com/seanmonstar/warp/blob/master/examples/unix_socket.rs

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() {
    #[cfg(target_os = "linux")]
    use tokio::net::UnixListener;
    #[cfg(target_os = "linux")]
    use tokio_stream::wrappers::UnixListenerStream;

    use salvo_core::routing::Router;
    use salvo_core::Server;
    use salvo_extra::serve::StaticDir;

    let listener = UnixListener::bind("/tmp/salvo.sock").unwrap();
    let incoming = UnixListenerStream::new(listener);
    let router = Router::new().path("files/<*path>").get(StaticDir::new("./static"));
    Server::new(router).bind_incoming(incoming).await;
}

#[cfg(not(target_os = "linux"))]
#[tokio::main]
async fn main() {
    println!("please run this example in linux");
}
