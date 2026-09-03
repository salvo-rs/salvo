//! End-to-end check that [`Metrics`] emits what the OpenTelemetry HTTP
//! semantic conventions define.
//!
//! The middleware registers its instruments on the process-global meter
//! provider, so everything lives in one test function: a second test in this
//! binary would observe the same provider and the same exported measurements.

use std::collections::BTreeMap;

use opentelemetry::global;
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, SdkMeterProvider};
use salvo_core::http::Method;
use salvo_core::prelude::*;
use salvo_core::test::{RequestBuilder, TestClient};
use salvo_otel::Metrics;

/// Bucket boundaries the conventions recommend for the duration histogram.
const DURATION_BOUNDARIES: [f64; 14] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0,
];

/// One exported measurement, flattened into what the assertions care about.
#[derive(Debug)]
struct Point {
    attrs: BTreeMap<String, String>,
    /// `None` for a histogram, whose value is not asserted on.
    value: Option<i64>,
}

impl Point {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }
}

/// One exported instrument.
#[derive(Debug, Default)]
struct Instrument {
    unit: String,
    bounds: Vec<f64>,
    points: Vec<Point>,
}

fn attrs<'a>(pairs: impl Iterator<Item = &'a opentelemetry::KeyValue>) -> BTreeMap<String, String> {
    pairs
        .map(|kv| (kv.key.to_string(), kv.value.to_string()))
        .collect()
}

fn collect(exporter: &InMemoryMetricExporter) -> BTreeMap<String, Instrument> {
    let mut collected: BTreeMap<String, Instrument> = BTreeMap::new();
    for resource_metrics in exporter.get_finished_metrics().expect("exported metrics") {
        for scope_metrics in resource_metrics.scope_metrics() {
            assert_eq!(
                scope_metrics.scope().name(),
                "salvo-otel",
                "instruments are reported under the crate's own scope"
            );
            for metric in scope_metrics.metrics() {
                let entry = collected.entry(metric.name().to_owned()).or_default();
                entry.unit = metric.unit().to_owned();
                match metric.data() {
                    AggregatedMetrics::F64(MetricData::Histogram(histogram)) => {
                        for point in histogram.data_points() {
                            entry.bounds = point.bounds().collect();
                            entry.points.push(Point {
                                attrs: attrs(point.attributes()),
                                value: None,
                            });
                        }
                    }
                    AggregatedMetrics::U64(MetricData::Histogram(histogram)) => {
                        for point in histogram.data_points() {
                            entry.points.push(Point {
                                attrs: attrs(point.attributes()),
                                value: Some(point.sum() as i64),
                            });
                        }
                    }
                    AggregatedMetrics::I64(MetricData::Sum(sum)) => {
                        for point in sum.data_points() {
                            entry.points.push(Point {
                                attrs: attrs(point.attributes()),
                                value: Some(point.value()),
                            });
                        }
                    }
                    other => panic!("unexpected aggregation for {}: {other:?}", metric.name()),
                }
            }
        }
    }
    collected
}

#[handler]
async fn hello() -> &'static str {
    "Hello"
}

#[handler]
async fn boom() -> Result<(), StatusError> {
    Err(StatusError::internal_server_error())
}

#[tokio::test]
async fn test_metrics_follow_semantic_conventions() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());

    let router = Router::new()
        .hoop(Metrics::new())
        .push(Router::with_path("users/{id}").goal(hello))
        .push(Router::with_path("boom").goal(boom));
    let service = Service::new(router);

    TestClient::get("http://127.0.0.1:8698/users/42?sig=secret&token=abc")
        .send(&service)
        .await;
    TestClient::get("http://127.0.0.1:8698/boom")
        .send(&service)
        .await;
    // An absolute-form request target lets the client pick the scheme, so it
    // must not reach a metric dimension verbatim.
    RequestBuilder::new("salvo-is-great://127.0.0.1:8698/users/42", Method::GET)
        .send(&service)
        .await;

    provider.force_flush().expect("flush");
    let collected = collect(&exporter);

    assert_eq!(
        collected.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "http.server.active_requests",
            "http.server.request.duration",
            "http.server.response.body.size",
        ],
        "only the conventions' instruments are emitted, and no request or error counter"
    );

    let duration = &collected["http.server.request.duration"];
    assert_eq!(
        duration.unit, "s",
        "the conventions measure duration in seconds"
    );
    assert_eq!(
        duration.bounds, DURATION_BOUNDARIES,
        "the histogram uses the conventions' recommended buckets"
    );

    for point in &duration.points {
        assert!(
            !point.attrs.contains_key("url.full"),
            "the request URI must not become a metric dimension: {:?}",
            point.attrs
        );
        assert!(
            !point.attrs.contains_key("exception.message"),
            "an error message must not become a metric dimension: {:?}",
            point.attrs
        );
        assert_eq!(point.attr("http.request.method"), Some("GET"));
        assert_eq!(point.attr("network.protocol.version"), Some("1.1"));
        assert!(
            matches!(point.attr("url.scheme"), Some("http" | "_OTHER")),
            "a scheme a salvo server cannot have spoken is reported as _OTHER, got {:?}",
            point.attr("url.scheme")
        );
    }
    assert!(
        duration
            .points
            .iter()
            .any(|point| point.attr("url.scheme") == Some("_OTHER")),
        "the invented scheme was collapsed, not dropped"
    );

    let ok = duration
        .points
        .iter()
        .find(|point| {
            point.attr("http.route") == Some("/users/{id}")
                && point.attr("url.scheme") == Some("http")
        })
        .expect("the matched route template is reported, not the requested path");
    assert_eq!(ok.attr("http.response.status_code"), Some("200"));
    assert_eq!(
        ok.attr("error.type"),
        None,
        "a successful request carries no error class"
    );

    let failed = duration
        .points
        .iter()
        .find(|point| point.attr("http.route") == Some("/boom"))
        .expect("the failing route is reported");
    assert_eq!(failed.attr("http.response.status_code"), Some("500"));
    assert_eq!(
        failed.attr("error.type"),
        Some("500"),
        "the conventions use the status code as the error class"
    );

    let active = &collected["http.server.active_requests"];
    assert_eq!(active.unit, "{request}");
    assert!(!active.points.is_empty(), "the gauge was actually recorded");
    assert_eq!(
        active
            .points
            .iter()
            .map(|point| point.value)
            .sum::<Option<i64>>(),
        Some(0),
        "every increment is matched by a decrement once the request finishes"
    );
    for point in &active.points {
        assert_eq!(
            point.attrs.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["http.request.method", "url.scheme"],
            "the gauge carries only the attributes the conventions list for it"
        );
    }

    let body = &collected["http.server.response.body.size"];
    assert_eq!(body.unit, "By");
    assert!(
        body.points
            .iter()
            .all(|point| point.attr("http.route") == Some("/users/{id}")),
        "the error response body is written after middleware returns, so it is not measured"
    );
    assert_eq!(
        body.points
            .iter()
            .map(|point| point.value)
            .sum::<Option<i64>>(),
        Some(10),
        "two responses of \"Hello\", five bytes each"
    );
}
