use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::{propagation::TraceContextPropagator, trace::Tracer, Resource};
use salvo::otel::{Metrics, Tracing};
use salvo::prelude::*;

mod exporter;
use exporter::Exporter;

fn init_tracer() -> Tracer {
    global::set_text_map_propagator(TraceContextPropagator::new());
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(
            opentelemetry_sdk::trace::config()
                .with_resource(Resource::new(vec![KeyValue::new("service.name", "server2")])),
        )
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .install_batch(opentelemetry_sdk::runtime::Tokio)
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
    let acceptor = TcpListener::new("0.0.0.0:5801").bind().await;
    Server::new(acceptor).serve(router).await;
}
