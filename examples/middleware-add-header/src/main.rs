use salvo::http::header::{self, HeaderValue};
use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[handler]
async fn add_header(res: &mut Response) {
    res.headers_mut()
        .insert(header::SERVER, HeaderValue::from_static("Salvo"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().hoop(add_header).get(hello);
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
