use salvo::prelude::*;
use salvo::otel::{Tracing, Metrics};

mod exporter;
use exporter::Exporter;

#[handler]
async fn index() -> String {
    let tracer = depot.obtain::<Arc<Tracing>>().unwrap();
    let mut span = tracer
        .span_builder("request/server2")
        .with_kind(SpanKind::Client)
        .start(tracer.0);
    let cx = Context::current_with_span(span);
    let client = Client::new();

    let req = {
        let mut req = reqwest::Request::new(Method::GET, Url::from_str("http://localhost:5801/api2").unwrap());
        global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&cx, &mut HeaderInjector(req.headers_mut()))
        });
        *req.body_mut() = Some((body + "server1\n").into());
        req
    };

    let fut = async move {
        let cx = Context::current();
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

    let router = Router::new()
        .hoop(affix::inject(tracer))
        .hoop(Metrics::new())
        .hoop(Tracing::new())
        .push(Router::with_path("api1").get(index))
        .push(Router::with_path("metrics").get(Exporter::new()));
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
