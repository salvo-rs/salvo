use salvo::otel::{Metrics, Tracing};
use salvo::prelude::*;

mod exporter;
use exporter::Exporter;

#[handler]
async fn index(req: &mut Request) -> String {
    format!("Body: {}" str::from_utf8(req.payload().unwrap()).unwrap())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .hoop(Metrics::new())
        .hoop(Tracing::new())
        .push(Router::with_path("api1").get(index))
        .push(Router::with_path("metrics").get(Exporter::new()));
    let acceptor = TcpListener::new("127.0.0.1:5801").bind().await;
    Server::new(acceptor).serve(router).await;
}
