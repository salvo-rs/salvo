use listenfd::ListenFd;
use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    
    let router = Router::new().get(hello);

    let mut listenfd = ListenFd::from_env();
    // if listenfd doesn't take a TcpListener (i.e. we're not running via
    // the command above), we fall back to explicitly binding to a given
    // host:port.
    let server = if let Some(l) = listenfd.take_tcp_listener(0).unwrap() {
        hyper::server::Server::from_tcp(l).unwrap()
    } else {
        hyper::server::Server::bind(&([127, 0, 0, 1], 7878).into())
    };

    server.serve(Service::new(router)).await.unwrap();
}
