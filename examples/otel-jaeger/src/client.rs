use std::error::Error;
use std::str::FromStr;

use opentelemetry::{
    Context, KeyValue, global,
    trace::{FutureExt, TraceContextExt, Tracer as _},
};
use opentelemetry_http::HeaderInjector;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use reqwest::{Client, Method, Url};

fn init_tracer_provider() -> SdkTracerProvider {
    global::set_text_map_propagator(TraceContextPropagator::new());
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:14268/api/traces")
        .build()
        .expect("failed to create exporter");
    SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let provider = init_tracer_provider();
    global::set_tracer_provider(provider.clone());
    let client = Client::new();
    let span = global::tracer("example-opentelemetry/client").start("request/server1");
    let cx = Context::current_with_span(span);

    let req = {
        let mut req = reqwest::Request::new(
            Method::GET,
            Url::from_str("http://localhost:5800/api1").unwrap(),
        );
        global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&cx, &mut HeaderInjector(req.headers_mut()));
            println!("{:?}", req.headers_mut());
        });
        *req.body_mut() = Some("client\n".into());
        req
    };

    async move {
        let cx = Context::current();
        let span = cx.span();

        span.add_event("Send request to server1".to_string(), vec![]);
        let resp = client.execute(req).await.unwrap();
        span.add_event(
            "Got response from server1!".to_string(),
            vec![KeyValue::new("status", resp.status().to_string())],
        );
        println!("{}", resp.text().await.unwrap());
    }
    .with_context(cx)
    .await;

    provider.shutdown()?;
    Ok(())
}
