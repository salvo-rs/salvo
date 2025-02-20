use opentelemetry::global;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::{Resource, propagation::TraceContextPropagator};
use salvo::otel::{Metrics, Tracing};
use salvo::prelude::*;

mod exporter;
use exporter::Exporter;

fn init_tracer_provider() -> SdkTracerProvider {
    global::set_text_map_propagator(TraceContextPropagator::new());
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()
        .expect("failed to create exporter");
    SdkTracerProvider::builder()
        .with_resource(Resource::builder().with_service_name("server2").build())
        .with_batch_exporter(exporter)
        .build()
}

#[handler]
async fn index(req: &mut Request) -> String {
    format!(
        "Body: {}",
        std::str::from_utf8(req.payload().await.unwrap()).unwrap()
    )
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let tracer = init_tracer_provider().tracer("app");
    let router = Router::new()
        .hoop(Metrics::new())
        .hoop(Tracing::new(tracer))
        .push(Router::with_path("api2").get(index))
        .push(Router::with_path("metrics").get(Exporter::new()));
    let acceptor = TcpListener::new("0.0.0.0:5801").bind().await;
    Server::new(acceptor).serve(router).await;
}
