use salvo::logging::Logger;
use salvo::prelude::*;

#[handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().hoop(Logger).get(hello_world);

    Server::new(TcpListener::bind("127.0.0.1:7878").await)
        .serve(router)
        .await;
}
