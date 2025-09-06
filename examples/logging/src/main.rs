use salvo::logging::Logger;
use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[handler]
async fn error() -> StatusError {
    StatusError::bad_request().brief("Bad request error").detail("The request was malformed.")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello).push(Router::with_path("error").get(error));
    let service = Service::new(router).hoop(Logger::new());

    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(service).await;
}
