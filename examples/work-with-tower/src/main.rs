use salvo::prelude::*;
use tower::limit::ConcurrencyLimit;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello);
    let server = ConcurrencyLimit::new(Service::new(router), 20);

    
    let _ = hyper::server::Server::bind(&std::net::SocketAddr::from(([127, 0, 0, 1], 7878)))
        .serve(server)
        .await;
}
