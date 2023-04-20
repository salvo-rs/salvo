
use std::net::SocketAddr;

use salvo::prelude::*;
use tower::limit::ConcurrencyLimit;
use hyper::server::conn::http1;
use hyper::service::service_fn;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello);
    let server = ConcurrencyLimit::new(Service::new(router), 20);

    let addr: SocketAddr = ([127, 0, 0, 1], 5800).into();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Listening on http://{}", addr);
    loop {
        let (stream, _) = listener.accept().await?;

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new().serve_connection(stream, server).await {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}
