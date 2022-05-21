use salvo::http::header::{self, HeaderValue};
use salvo::prelude::*;

#[fn_handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[fn_handler]
async fn add_header(res: &mut Response) {
    res.headers_mut()
        .insert(header::SERVER, HeaderValue::from_static("Salvo"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(route()).await;
}

fn route() -> Router {
    Router::new().hoop(add_header).get(hello_world)
}
