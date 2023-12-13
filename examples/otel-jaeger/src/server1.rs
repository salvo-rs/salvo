use std::str::FromStr;
use std::sync::Arc;

use opentelemetry_sdk::{propagation::TraceContextPropagator, trace::Tracer};
use opentelemetry::trace::{FutureExt, SpanKind, TraceContextExt, Tracer as _};
use opentelemetry::{global, KeyValue};
use opentelemetry_http::HeaderInjector;
use reqwest::{Url, Method, Client};
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
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .unwrap()
}

#[handler]
async fn index(req: &mut Request, depot: &mut Depot) -> String {
    let tracer = depot.obtain::<Arc<Tracer>>().unwrap();
    let span = tracer
        .span_builder("request/server2")
        .with_kind(SpanKind::Client)
        .start(&**tracer);
    let cx = opentelemetry::Context::current_with_span(span);
    let client = Client::new();

    let body = std::str::from_utf8(req.payload().await.unwrap()).unwrap();
    let req = {
        let mut req = reqwest::Request::new(Method::GET, Url::from_str("http://localhost:5801/api2").unwrap());
        global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&cx, &mut HeaderInjector(req.headers_mut()))
        });
        *req.body_mut() = Some(format!("{body} server1\n").into());
        req
    };

    let fut = async move {
        let cx = opentelemetry::Context::current();
        let span = cx.span();

        span.add_event("Send request to server2".to_string(), vec![]);
        let resp = client.execute(req).await.unwrap();
        span.add_event(
            "Got response from server2!".to_string(),
            vec![KeyValue::new("status", resp.status().to_string())],
        );
        resp
    }
    .with_context(cx);

    fut.await.text().await.unwrap()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let tracer = init_tracer();
    let router = Router::new()
        .hoop(affix::inject(Arc::new(tracer.clone())))
        .hoop(Metrics::new())
        .hoop(Tracing::new(tracer))
        .push(Router::with_path("api1").get(index))
        .push(Router::with_path("metrics").get(Exporter::new()));
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
