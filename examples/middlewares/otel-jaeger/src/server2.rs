use opentelemetry::global;
use opentelemetry::sdk::{propagation::TraceContextPropagator, trace::Tracer};
use salvo::otel::{Metrics, Tracing};
use salvo::prelude::*;

mod exporter;
use exporter::Exporter;

fn init_tracer() -> Tracer {
    global::set_text_map_propagator(TraceContextPropagator::new());
    opentelemetry_jaeger::new_collector_pipeline()
        .with_service_name("salvo")
        .with_endpoint("http://localhost:14268/api/traces")
        .with_hyper()
        .install_batch(opentelemetry::runtime::Tokio)
        .unwrap()
}

#[handler]
async fn index(req: &mut Request) -> String {
    format!("Body: {}", std::str::from_utf8(req.payload().await.unwrap()).unwrap())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let tracer = init_tracer();
    let router = Router::new()
        .hoop(Metrics::new())
        .hoop(Tracing::new(tracer))
        .push(Router::with_path("api2").get(index))
        .push(Router::with_path("metrics").get(Exporter::new()));
    let acceptor = TcpListener::new("127.0.0.1:5801").bind().await;
    Server::new(acceptor).serve(router).await;
}
