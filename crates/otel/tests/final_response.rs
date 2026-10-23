//! Both instruments must observe the response after all Service processing.
use std::time::Duration;

use opentelemetry::trace::{Status, TracerProvider as _};
use opentelemetry::{KeyValue, global};
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, SdkMeterProvider};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use salvo_core::catcher::Catcher;
use salvo_core::prelude::*;
use salvo_core::test::TestClient;
use salvo_extra::catch_panic::CatchPanic;
use salvo_extra::timeout::Timeout;
use salvo_extra::tower_compat::{FlowCtrlService, TowerLayerCompat};
use salvo_otel::{Metrics, Tracing};

#[derive(Clone, Copy)]
enum TowerMode {
    ShortCircuit,
    Error,
    PassThrough,
}

struct TowerProbe {
    inner: FlowCtrlService,
    mode: TowerMode,
}

impl tower::Service<salvo_core::hyper::Request<salvo_core::http::ReqBody>> for TowerProbe {
    type Response = salvo_core::hyper::Response<salvo_core::http::ResBody>;
    type Error = std::io::Error;
    type Future =
        std::pin::Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(
        &mut self,
        request: salvo_core::hyper::Request<salvo_core::http::ReqBody>,
    ) -> Self::Future {
        match self.mode {
            TowerMode::ShortCircuit => Box::pin(std::future::ready(Ok(
                salvo_core::hyper::Response::builder()
                    .status(429)
                    .body(salvo_core::http::ResBody::None)
                    .unwrap(),
            ))),
            TowerMode::Error => Box::pin(std::future::ready(Err(std::io::Error::other(
                "tower test error",
            )))),
            TowerMode::PassThrough => self.inner.call(request),
        }
    }
}

fn attr<'a>(attrs: impl Iterator<Item = &'a KeyValue>, name: &str) -> Option<String> {
    attrs
        .filter(|kv| kv.key.as_str() == name)
        .map(|kv| kv.value.to_string())
        .next()
}

#[handler]
async fn goal(req: &mut Request, res: &mut Response) {
    match req.uri().path() {
        "/timeout" | "/cancel" => std::future::pending::<()>().await,
        "/panic" => panic!("caught test panic"),
        "/catcher" | "/catcher-ok" => res.render(StatusError::not_found()),
        _ => res.render(Text::Json("{}")),
    }
}

#[handler]
async fn catch(req: &mut Request, res: &mut Response, ctrl: &mut FlowCtrl) {
    res.status_code(if req.uri().path() == "/catcher-ok" {
        StatusCode::OK
    } else {
        StatusCode::BAD_GATEWAY
    });
    res.render(Text::Plain("final catcher body"));
    ctrl.skip_rest();
}

// The global meter provider is changed only within this single test.
#[tokio::test]
async fn final_response_status_and_body_reach_metrics_and_tracing() {
    for (path, expected) in [
        ("/media", 415),
        ("/catcher", 502),
        ("/catcher-ok", 200),
        ("/timeout", 503),
        ("/panic", 500),
        ("/head", 200),
        ("/tower-short", 429),
        ("/tower-error", 500),
        ("/tower-pass", 200),
        ("/tower-error-head", 500),
    ] {
        let metrics = InMemoryMetricExporter::default();
        let meter = SdkMeterProvider::builder()
            .with_periodic_exporter(metrics.clone())
            .build();
        global::set_meter_provider(meter.clone());
        let spans = InMemorySpanExporter::default();
        let tracer = SdkTracerProvider::builder()
            .with_simple_exporter(spans.clone())
            .build();
        let mut router = Router::with_path("{**rest}")
            .hoop(CatchPanic::new())
            .hoop(Timeout::new(Duration::from_millis(20)))
            .hoop(Metrics::new())
            .hoop(Tracing::new(tracer.tracer("test")))
            .goal(goal);
        if path.starts_with("/tower-") {
            let mode = match path {
                "/tower-short" => TowerMode::ShortCircuit,
                "/tower-pass" => TowerMode::PassThrough,
                _ => TowerMode::Error,
            };
            router = router
                .hoop(tower::layer::layer_fn(move |inner| TowerProbe { inner, mode }).compat());
        }
        let mut service = Service::new(router);
        if path == "/media" {
            service = service.allowed_media_types(vec!["text/plain".parse().unwrap()]);
        } else if path.starts_with("/catcher") {
            service = service.catcher(Catcher::default().hoop(catch));
        }
        let request = if path.ends_with("head") {
            TestClient::head(format!("http://localhost{path}"))
        } else {
            TestClient::get(format!("http://localhost{path}"))
        };
        let res = request.send(&service).await;
        assert_eq!(res.status_code.unwrap().as_u16(), expected);
        let size = if path.ends_with("head") {
            None
        } else {
            res.body.size()
        };
        meter.force_flush().unwrap();
        tracer.force_flush().unwrap();
        let exported = spans.get_finished_spans().unwrap();
        assert_eq!(exported.len(), 1, "{path}: exactly one span");
        let span = &exported[0];
        assert_eq!(
            attr(span.attributes.iter(), "http.response.status_code"),
            Some(expected.to_string()),
            "{path}"
        );
        assert_eq!(
            attr(span.attributes.iter(), "http.response.body.size"),
            size.map(|size| size.to_string()),
            "{path}"
        );
        assert_eq!(
            span.events.events.last().unwrap().name,
            if expected >= 400 {
                "request.failure"
            } else {
                "request.success"
            }
        );
        assert_eq!(
            span.status,
            if expected >= 500 {
                Status::error("")
            } else {
                Status::Unset
            }
        );
        let mut duration_count = 0;
        let mut body_count = 0;
        let mut active = 0;
        for resource in metrics.get_finished_metrics().unwrap() {
            for scope in resource.scope_metrics() {
                for metric in scope.metrics() {
                    match metric.data() {
                        AggregatedMetrics::F64(MetricData::Histogram(histogram))
                            if metric.name() == "http.server.request.duration" =>
                        {
                            for point in histogram.data_points() {
                                assert_eq!(
                                    attr(point.attributes(), "http.response.status_code"),
                                    Some(expected.to_string()),
                                    "{path}"
                                );
                                duration_count += point.count();
                            }
                        }
                        AggregatedMetrics::U64(MetricData::Histogram(histogram))
                            if metric.name() == "http.server.response.body.size" =>
                        {
                            for point in histogram.data_points() {
                                assert_eq!(Some(point.sum()), size, "{path}");
                                body_count += point.count();
                            }
                        }
                        AggregatedMetrics::I64(MetricData::Sum(sum))
                            if metric.name() == "http.server.active_requests" =>
                        {
                            active += sum.data_points().map(|point| point.value()).sum::<i64>();
                        }
                        _ => {}
                    }
                }
            }
        }
        assert_eq!(duration_count, 1, "{path}: one duration observation");
        assert_eq!(
            body_count,
            u64::from(size.is_some()),
            "{path}: body observation only when known"
        );
        assert_eq!(active, 0, "{path}: active counter balanced");
        meter.shutdown().unwrap();
        tracer.shutdown().unwrap();
    }
    cancellation_and_direct_handler_fallback().await;
    tower_retained_request_cancellation().await;
}

type RetainedRequest =
    std::sync::Arc<std::sync::Mutex<Option<salvo_core::hyper::Request<salvo_core::http::ReqBody>>>>;

struct RetainingTower {
    request: RetainedRequest,
    entered: std::sync::Arc<tokio::sync::Notify>,
}

impl tower::Service<salvo_core::hyper::Request<salvo_core::http::ReqBody>> for RetainingTower {
    type Response = salvo_core::hyper::Response<salvo_core::http::ResBody>;
    type Error = std::io::Error;
    type Future = std::future::Pending<Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(
        &mut self,
        request: salvo_core::hyper::Request<salvo_core::http::ReqBody>,
    ) -> Self::Future {
        *self.request.lock().unwrap() = Some(request);
        self.entered.notify_one();
        std::future::pending()
    }
}

async fn tower_retained_request_cancellation() {
    let metrics = InMemoryMetricExporter::default();
    let meter = SdkMeterProvider::builder()
        .with_periodic_exporter(metrics.clone())
        .build();
    global::set_meter_provider(meter.clone());
    let spans = InMemorySpanExporter::default();
    let tracer = SdkTracerProvider::builder()
        .with_simple_exporter(spans.clone())
        .build();
    let retained = RetainedRequest::default();
    let entered = std::sync::Arc::new(tokio::sync::Notify::new());
    let layer = {
        let request = retained.clone();
        let entered = entered.clone();
        tower::layer::layer_fn(move |_| RetainingTower {
            request: request.clone(),
            entered: entered.clone(),
        })
    };
    let service = Service::new(
        Router::new()
            .hoop(Metrics::new())
            .hoop(Tracing::new(tracer.tracer("retained")))
            .hoop(layer.compat())
            .goal(goal),
    );
    let request =
        tokio::spawn(async move { TestClient::get("http://localhost/").send(&service).await });
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    meter.force_flush().unwrap();
    assert_eq!(active_requests(&metrics), 1);
    metrics.reset();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(
        retained.lock().unwrap().is_some(),
        "Tower still owns the original Request"
    );
    meter.force_flush().unwrap();
    tracer.force_flush().unwrap();
    assert_eq!(
        active_requests(&metrics),
        0,
        "Service cancellation owns cleanup even while Tower retains Request"
    );
    let cancelled = spans.get_finished_spans().unwrap();
    assert_eq!(cancelled.len(), 1);
    assert_eq!(
        attr(cancelled[0].attributes.iter(), "http.response.status_code"),
        None
    );
    retained.lock().unwrap().take();
    meter.shutdown().unwrap();
    tracer.shutdown().unwrap();
}

async fn cancellation_and_direct_handler_fallback() {
    let metrics = InMemoryMetricExporter::default();
    let meter = SdkMeterProvider::builder()
        .with_periodic_exporter(metrics.clone())
        .build();
    global::set_meter_provider(meter.clone());
    let spans = InMemorySpanExporter::default();
    let tracer = SdkTracerProvider::builder()
        .with_simple_exporter(spans.clone())
        .build();
    let service = Service::new(
        Router::with_path("{**rest}")
            .hoop(Metrics::new())
            .hoop(Tracing::new(tracer.tracer("test")))
            .goal(goal),
    );
    let mut request = Box::pin(TestClient::get("http://localhost/cancel").send(&service));
    std::future::poll_fn(|cx| {
        assert!(request.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    meter.force_flush().unwrap();
    assert_eq!(active_requests(&metrics), 1);
    metrics.reset();
    drop(request);
    meter.force_flush().unwrap();
    assert_eq!(
        active_requests(&metrics),
        0,
        "cancellation drops the captured active guard"
    );
    for resource in metrics.get_finished_metrics().unwrap() {
        for scope in resource.scope_metrics() {
            assert!(
                scope
                    .metrics()
                    .all(|metric| metric.name() != "http.server.request.duration"),
                "cancellation must not invent a successful response"
            );
        }
    }
    tracer.force_flush().unwrap();
    let cancelled = spans.get_finished_spans().unwrap();
    assert_eq!(cancelled.len(), 1, "cancelled span is released");
    assert_eq!(
        attr(cancelled[0].attributes.iter(), "http.response.status_code"),
        None
    );
    spans.reset();
    metrics.reset();

    // Preserve standalone Handler use: no Service is present to run observers.
    let mut req = TestClient::get("http://localhost/direct").build();
    let mut res = Response::new();
    let mut depot = Depot::new();
    let mut ctrl = FlowCtrl::new(vec![
        std::sync::Arc::new(Tracing::new(tracer.tracer("direct"))),
        std::sync::Arc::new(goal),
    ]);
    Metrics::new()
        .handle(&mut req, &mut depot, &mut res, &mut ctrl)
        .await;
    meter.force_flush().unwrap();
    tracer.force_flush().unwrap();
    assert_eq!(active_requests(&metrics), 0);
    let direct = spans.get_finished_spans().unwrap();
    assert_eq!(direct.len(), 1);
    assert_eq!(
        attr(direct[0].attributes.iter(), "http.response.status_code"),
        Some("200".into())
    );
    let mut count = 0;
    for resource in metrics.get_finished_metrics().unwrap() {
        for scope in resource.scope_metrics() {
            for metric in scope.metrics() {
                if metric.name() == "http.server.request.duration"
                    && let AggregatedMetrics::F64(MetricData::Histogram(histogram)) = metric.data()
                {
                    count += histogram
                        .data_points()
                        .map(|point| point.count())
                        .sum::<u64>();
                }
            }
        }
    }
    assert_eq!(count, 1);
    meter.shutdown().unwrap();
    tracer.shutdown().unwrap();
}

fn active_requests(exporter: &InMemoryMetricExporter) -> i64 {
    let mut value = 0;
    for resource in exporter.get_finished_metrics().unwrap() {
        for scope in resource.scope_metrics() {
            for metric in scope.metrics() {
                if metric.name() == "http.server.active_requests"
                    && let AggregatedMetrics::I64(MetricData::Sum(sum)) = metric.data()
                {
                    value += sum.data_points().map(|point| point.value()).sum::<i64>();
                }
            }
        }
    }
    value
}
