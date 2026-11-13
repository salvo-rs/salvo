//! End-to-end checks that [`Tracing`] produces the span names and attributes
//! the OpenTelemetry HTTP semantic conventions define.

use std::collections::BTreeMap;

use opentelemetry::trace::{Status, TracerProvider as _};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SimpleSpanProcessor};
use salvo_core::http::Method;
use salvo_core::prelude::*;
use salvo_core::test::{RequestBuilder, TestClient};
use salvo_otel::Tracing;

/// One exported span, flattened into what the assertions care about.
struct Span {
    name: String,
    attrs: BTreeMap<String, String>,
    status: Status,
}

impl Span {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }
}

#[handler]
async fn hello() -> &'static str {
    "Hello"
}

#[handler]
async fn boom() -> Result<(), StatusError> {
    Err(StatusError::internal_server_error())
}

/// Runs `request` through the service `build_service` assembles around a
/// tracer, and returns the single span it produced.
async fn trace_request<F>(request: RequestBuilder, build_service: F) -> Span
where
    F: FnOnce(&SdkTracerProvider) -> Service,
{
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_span_processor(SimpleSpanProcessor::new(exporter.clone()))
        .build();
    let service = build_service(&provider);

    request.send(&service).await;
    provider.force_flush().expect("flush");

    let mut spans = exporter.get_finished_spans().expect("exported spans");
    assert_eq!(spans.len(), 1, "one server span per request");
    let span = spans.remove(0);
    Span {
        name: span.name.into_owned(),
        attrs: span
            .attributes
            .iter()
            .map(|kv| (kv.key.to_string(), kv.value.to_string()))
            .collect(),
        status: span.status,
    }
}

/// The service most of these tests run against: routed, tracing on the router.
fn routed(provider: &SdkTracerProvider) -> Service {
    Service::new(
        Router::new()
            .hoop(Tracing::new(provider.tracer("test")))
            .push(Router::with_path("users/{id}").goal(hello))
            .push(Router::with_path("boom").goal(boom)),
    )
}

#[tokio::test]
async fn test_span_reports_route_and_split_url() {
    let span = trace_request(
        TestClient::get("http://127.0.0.1:8698/users/42?s%69g=secret&token=abc"),
        routed,
    )
    .await;

    assert_eq!(
        span.name,
        if cfg!(feature = "matched-path") {
            "GET /users/{id}"
        } else {
            "GET"
        },
        "the span name follows the configured route behavior"
    );
    assert_eq!(span.attr("http.request.method"), Some("GET"));
    assert_eq!(
        span.attr("http.route"),
        if cfg!(feature = "matched-path") {
            Some("/users/{id}")
        } else {
            None
        }
    );
    assert_eq!(span.attr("url.path"), Some("/users/42"));
    assert_eq!(
        span.attr("url.query"),
        None,
        "query strings are omitted by default"
    );
    assert_eq!(span.attr("url.scheme"), Some("http"));
    assert_eq!(span.attr("network.protocol.version"), Some("1.1"));
    assert_eq!(span.attr("http.response.status_code"), Some("200"));
    assert_eq!(span.attr("http.response.body.size"), Some("5"));
    assert_eq!(span.attr("error.type"), None);
    assert_eq!(span.status, Status::Unset);

    assert_eq!(
        span.attr("url.full"),
        None,
        "the full URI is no longer recorded"
    );
    for key in [
        "telemetry.sdk.name",
        "telemetry.sdk.version",
        "telemetry.sdk.language",
    ] {
        assert_eq!(
            span.attr(key),
            None,
            "{key} describes the resource, not a span"
        );
    }
}

#[tokio::test]
async fn test_span_normalizes_unknown_method() {
    let propfind = Method::from_bytes(b"PROPFIND").expect("valid method");
    let span = trace_request(
        RequestBuilder::new("http://127.0.0.1:8698/users/42", propfind),
        routed,
    )
    .await;

    assert_eq!(span.attr("http.request.method"), Some("_OTHER"));
    assert_eq!(
        span.attr("http.request.method_original"),
        Some("PROPFIND"),
        "the value the client sent is kept alongside the normalized one"
    );
    assert_eq!(
        span.name,
        if cfg!(feature = "matched-path") {
            "HTTP /users/{id}"
        } else {
            "HTTP"
        }
    );
}

#[tokio::test]
async fn test_span_marks_server_error() {
    let span = trace_request(TestClient::get("http://127.0.0.1:8698/boom"), routed).await;

    assert_eq!(span.attr("http.response.status_code"), Some("500"));
    assert_eq!(span.attr("error.type"), Some("500"));
    assert!(
        span.attr("http.response.body.size")
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 0,
        "the finalized catcher body is observed"
    );
    assert_eq!(
        span.status,
        Status::error(""),
        "an HTTP server error fails the span without a redundant status description"
    );
}

#[tokio::test]
async fn test_span_without_matched_route() {
    let span = trace_request(
        TestClient::get("http://127.0.0.1:8698/nowhere"),
        |provider| {
            // The router hoop never runs when nothing matches, so trace from the service.
            Service::new(Router::new().push(Router::with_path("users/{id}").goal(hello)))
                .hoop(Tracing::new(provider.tracer("test")))
        },
    )
    .await;

    assert_eq!(
        span.name, "GET",
        "with no route to name it after, the span is named for the method alone"
    );
    assert_eq!(
        span.attr("http.route"),
        None,
        "the conventions leave the attribute out rather than filling it with the path"
    );
    assert_eq!(span.attr("url.path"), Some("/nowhere"));
    assert_eq!(span.attr("http.response.status_code"), Some("404"));
    assert_eq!(
        span.attr("error.type"),
        None,
        "a client error is a valid outcome for a server span"
    );
    assert!(
        span.attr("http.response.body.size")
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 0,
        "the finalized 404 body is observed"
    );
    assert_eq!(span.status, Status::Unset);
}

#[tokio::test]
async fn test_span_names_root_route() {
    let span = trace_request(TestClient::get("http://127.0.0.1:8698/"), |provider| {
        // A goal mounted at the router root matches with no path parts, which
        // salvo reports the same way it reports a request that matched nothing.
        Service::new(
            Router::new()
                .hoop(Tracing::new(provider.tracer("test")))
                .goal(hello),
        )
    })
    .await;

    assert_eq!(
        span.attr("http.route"),
        if cfg!(feature = "matched-path") {
            Some("/")
        } else {
            None
        }
    );
    assert_eq!(
        span.name,
        if cfg!(feature = "matched-path") {
            "GET /"
        } else {
            "GET"
        }
    );
    assert_eq!(span.attr("http.response.status_code"), Some("200"));
}

#[tokio::test]
async fn test_span_without_root_route() {
    let span = trace_request(TestClient::get("http://127.0.0.1:8698/"), |provider| {
        // No root goal, so `/` matches nothing — and salvo reports that with the
        // same empty matched path a root goal would have produced.
        Service::new(Router::new().push(Router::with_path("users/{id}").goal(hello)))
            .hoop(Tracing::new(provider.tracer("test")))
    })
    .await;

    assert_eq!(
        span.attr("http.route"),
        None,
        "an unmatched request for `/` is not the root route"
    );
    assert_eq!(span.name, "GET");
    assert_eq!(
        span.attr("http.response.status_code"),
        Some("404"),
        "a root path without a reachable goal is not found"
    );
}

#[tokio::test]
async fn query_credentials_are_omitted_by_default() {
    let span = trace_request(
        TestClient::get(concat!(
            "http://127.0.0.1:8698/users/42?",
            "access_token=access-secret&access_token=repeat-secret&",
            "%61pi_key=api-secret&password=password-secret&code=oauth-secret&",
            "custom_credential=custom-secret&token=token-secret&page=2"
        )),
        routed,
    )
    .await;
    assert_eq!(span.attr("url.query"), None);
    let exported = format!("{} {:?}", span.name, span.attrs);
    for secret in [
        "access-secret",
        "repeat-secret",
        "api-secret",
        "password-secret",
        "oauth-secret",
        "custom-secret",
        "token-secret",
    ] {
        assert!(!exported.contains(secret), "leaked {secret}");
    }
}

#[tokio::test]
async fn query_allowlist_keeps_only_explicit_non_sensitive_fields() {
    let span = trace_request(
        TestClient::get(concat!(
            "http://127.0.0.1:8698/?page=1&access_token=access-secret&",
            "%70age=2&api_key=api-secret&code=oauth-secret&",
            "custom_credential=custom-secret&sort=a%26token%3Db&",
            "token=token-secret&sort+order=ascending&Page=case-secret&",
            "%FF=invalid-secret&debug"
        )),
        |provider| {
            Service::new(
                Router::new()
                    .hoop(Tracing::new(provider.tracer("test")).query_fields([
                        "page",
                        "sort",
                        "sort order",
                        "debug",
                    ]))
                    .goal(hello),
            )
        },
    )
    .await;
    assert_eq!(
        span.attr("url.query"),
        Some("page=1&%70age=2&sort=a%26token%3Db&sort+order=ascending&debug")
    );
    let exported = format!("{} {:?}", span.name, span.attrs);
    for secret in [
        "access-secret",
        "api-secret",
        "oauth-secret",
        "custom-secret",
        "token-secret",
        "case-secret",
        "invalid-secret",
    ] {
        assert!(!exported.contains(secret), "leaked {secret}");
    }
}

#[tokio::test]
async fn listed_signature_fields_remain_redacted() {
    let span = trace_request(
        TestClient::get(concat!(
            "http://127.0.0.1:8698/?s%69g=first-secret&sig=repeat-secret&",
            "Signature=signature-secret&AWSAccessKeyId=aws-secret&",
            "X-Goog-%53ignature=google-secret&page=2"
        )),
        |provider| {
            Service::new(
                Router::new()
                    .hoop(Tracing::new(provider.tracer("test")).query_fields([
                        "sig",
                        "Signature",
                        "AWSAccessKeyId",
                        "X-Goog-Signature",
                        "page",
                    ]))
                    .goal(hello),
            )
        },
    )
    .await;
    assert_eq!(
        span.attr("url.query"),
        Some(concat!(
            "s%69g=REDACTED&sig=REDACTED&Signature=REDACTED&",
            "AWSAccessKeyId=REDACTED&X-Goog-%53ignature=REDACTED&page=2"
        ))
    );
    assert!(!format!("{:?}", span.attrs).contains("secret"));
}

#[tokio::test]
async fn empty_or_unmatched_query_allowlist_omits_attribute() {
    for fields in [vec!["page"], vec![]] {
        let span = trace_request(
            TestClient::get("http://127.0.0.1:8698/?token=secret"),
            |provider| {
                Service::new(
                    Router::new()
                        .hoop(
                            Tracing::new(provider.tracer("test"))
                                .query_fields(["token"])
                                .query_fields(fields),
                        )
                        .goal(hello),
                )
            },
        )
        .await;
        assert_eq!(span.attr("url.query"), None);
    }
}
