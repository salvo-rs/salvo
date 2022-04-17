use salvo::prelude::*;
use tower::limit::ConcurrencyLimit;

#[fn_handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello_world);
    let server = ConcurrencyLimit::new(Service::new(router), 20);

    tracing::info!("Listening on http://0.0.0.0:7878");
    let _ = hyper::server::Server::bind(&std::net::SocketAddr::from(([127, 0, 0, 1], 7878)))
        .serve(server)
        .await;
}
