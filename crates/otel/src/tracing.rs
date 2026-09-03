use std::fmt::{self, Debug, Formatter};

use opentelemetry::trace::{FutureExt, Span, SpanKind, Status, TraceContextExt, Tracer};
use opentelemetry::{Context, KeyValue, global};
use opentelemetry_http::HeaderExtractor;
use opentelemetry_semantic_conventions::attribute;
use salvo_core::http::headers::{HeaderMap, HeaderName, HeaderValue};
use salvo_core::prelude::*;

use crate::semconv::{self, KnownMethods, OTHER};

/// Span name used in place of the method when the method is not a known one.
const OTHER_METHOD_SPAN_NAME: &str = "HTTP";

/// Middleware creating a server span per request, with the attributes the
/// OpenTelemetry [HTTP semantic conventions] define.
///
/// The span is named `{method} {route}` — `GET /users/{id}` — falling back to
/// the method alone when no route matched, and carries `http.request.method`,
/// `url.path`, `url.query`, `url.scheme`, `http.route`, `client.address`,
/// `client.port`, `network.protocol.version`, `http.response.status_code`,
/// `http.response.body.size` and, for a server error, `error.type`.
///
/// The request target is reported split into `url.path` and `url.query` rather
/// than as one `url.full`, and the query has the values of well-known
/// credential-bearing keys (`sig`, `Signature`, `AWSAccessKeyId`,
/// `X-Goog-Signature`) replaced with `REDACTED`, as the conventions require.
///
/// `http.route` requires the `matched-path` feature, which is enabled by
/// default. Through the `salvo` crate, enable its own `matched-path` feature
/// (also on by default) to turn this one on. A request that matched no route
/// carries no `http.route` and is named for its method alone.
///
/// [HTTP semantic conventions]: https://opentelemetry.io/docs/specs/semconv/http/http-spans/
pub struct Tracing<T> {
    tracer: T,
    known_methods: Option<KnownMethods>,
}

impl<T> Tracing<T> {
    /// Create `Tracing` middleware with `tracer`.
    pub fn new(tracer: T) -> Self {
        Self {
            tracer,
            known_methods: None,
        }
    }

    /// Replaces the set of methods reported verbatim in `http.request.method`.
    ///
    /// By default only the methods RFC 9110 defines, plus `PATCH` from RFC 5789,
    /// are reported as themselves; any other method becomes `_OTHER`, with the
    /// value the client sent recorded in `http.request.method_original`. An
    /// application that serves further methods — WebDAV's `PROPFIND`, say — can
    /// list them here.
    ///
    /// The set replaces the default rather than extending it, so include every
    /// method that should be reported verbatim. Names are matched
    /// case-sensitively.
    #[must_use]
    pub fn with_known_methods<I, S>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.known_methods = Some(semconv::known_methods(methods));
        self
    }
}

impl<T> Debug for Tracing<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tracing")
            .field("tracer", &self.tracer)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl<T> Handler for Tracing<T>
where
    T: Tracer + Sync + Send + 'static,
    T::Span: Send + Sync + 'static,
{
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        // TODO: Will remove after opentelemetry_http updated
        let mut headers = HeaderMap::with_capacity(req.headers().len());
        for (name, value) in req.headers() {
            // The names/values come from a `HeaderMap`, so they were already
            // valid for `HeaderName`/`HeaderValue`. Skip silently in the
            // unlikely event that round-tripping through bytes fails — losing
            // a propagated header is preferable to panicking the request task.
            let Ok(name) = HeaderName::from_bytes(name.as_ref()) else {
                continue;
            };
            let Ok(value) = HeaderValue::from_bytes(value.as_ref()) else {
                continue;
            };
            // Use `append` so multi-value headers (e.g. `baggage`) keep all
            // their values; `insert` would collapse them to the last one and
            // produce an incomplete parent context for `HeaderExtractor`.
            headers.append(name, value);
        }

        let parent_cx = global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor(&headers))
        });

        let mut attributes = Vec::with_capacity(10);

        // A method the instrumentation does not know is reported as `_OTHER`, so
        // it cannot widen the value space of derived metrics; the value sent is
        // kept in `http.request.method_original`.
        let span_method = if let Some(method) =
            semconv::known_method_value(req.method(), self.known_methods.as_ref())
        {
            let span_method = req.method().as_str().to_owned();
            attributes.push(KeyValue::new(attribute::HTTP_REQUEST_METHOD, method));
            span_method
        } else {
            attributes.push(KeyValue::new(attribute::HTTP_REQUEST_METHOD, OTHER));
            attributes.push(KeyValue::new(
                attribute::HTTP_REQUEST_METHOD_ORIGINAL,
                req.method().as_str().to_owned(),
            ));
            OTHER_METHOD_SPAN_NAME.to_owned()
        };

        attributes.push(KeyValue::new(
            attribute::URL_PATH,
            req.uri().path().to_owned(),
        ));
        if let Some(query) = req.uri().query() {
            attributes.push(KeyValue::new(
                attribute::URL_QUERY,
                semconv::redact_query(query).into_owned(),
            ));
        }
        attributes.push(KeyValue::new(
            attribute::URL_SCHEME,
            semconv::scheme_value(req.scheme()),
        ));

        // The service resolves the route before running middleware, so the span
        // can be named after it.
        #[cfg(feature = "matched-path")]
        let route = semconv::route_value(req);
        #[cfg(not(feature = "matched-path"))]
        let route: Option<String> = None;
        let span_name = match &route {
            Some(route) => format!("{span_method} {route}"),
            None => span_method,
        };
        if let Some(route) = route {
            attributes.push(KeyValue::new(attribute::HTTP_ROUTE, route));
        }

        let remote_addr = req.remote_addr();
        if let Some(addr) = remote_addr.ip() {
            attributes.push(KeyValue::new(attribute::CLIENT_ADDRESS, addr.to_string()));
        }
        if let Some(port) = remote_addr.port() {
            attributes.push(KeyValue::new(attribute::CLIENT_PORT, i64::from(port)));
        }
        if let Some(version) = semconv::protocol_version(req.version()) {
            attributes.push(KeyValue::new(attribute::NETWORK_PROTOCOL_VERSION, version));
        }

        let mut span = self
            .tracer
            .span_builder(span_name)
            .with_kind(SpanKind::Server)
            .with_attributes(attributes)
            .start_with_context(&self.tracer, &parent_cx);

        span.add_event("request.started".to_owned(), vec![]);

        async move {
            ctrl.call_next(req, depot, res).await;
            let cx = Context::current();
            let span = cx.span();

            let status = res.status_code.unwrap_or_else(|| {
                tracing::info!("[otel::Tracing] Treat status_code=none as 200(OK)");
                StatusCode::OK
            });
            let event = if status.is_client_error() || status.is_server_error() {
                "request.failure"
            } else {
                "request.success"
            };
            span.add_event(event.to_owned(), vec![]);
            span.set_attribute(KeyValue::new(
                attribute::HTTP_RESPONSE_STATUS_CODE,
                i64::from(status.as_u16()),
            ));
            if let Some(size) = semconv::final_body_size(req, res, status) {
                span.set_attribute(KeyValue::new(
                    attribute::HTTP_RESPONSE_BODY_SIZE,
                    size as i64,
                ));
            }
            // Only a server error makes a server span a failure: a `4xx` is a
            // valid outcome the caller asked for.
            if status.is_server_error() {
                span.set_attribute(KeyValue::new(
                    attribute::ERROR_TYPE,
                    status.as_str().to_owned(),
                ));
                span.set_status(Status::error(
                    status.canonical_reason().unwrap_or_default().to_owned(),
                ));
            }
        }
        .with_context(Context::current_with_span(span))
        .await
    }
}

#[cfg(test)]
mod tests {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry::trace::noop::NoopTracerProvider;
    use salvo_core::test::{ResponseExt, TestClient};
    use salvo_core::{Depot, FlowCtrl, Request, Response};

    use super::*;

    #[tokio::test]
    async fn test_tracing_handler() {
        let tracer = NoopTracerProvider::new().tracer("test");
        let handler = Tracing::new(tracer);

        let mut req = Request::new();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut ctrl = FlowCtrl::new(vec![]);

        handler
            .handle(&mut req, &mut depot, &mut res, &mut ctrl)
            .await;
    }

    #[tokio::test]
    async fn test_tracing_routed_request() {
        #[handler]
        async fn hello() -> &'static str {
            "Hello"
        }

        let tracer = NoopTracerProvider::new().tracer("test");
        let router = Router::new()
            .hoop(Tracing::new(tracer).with_known_methods(["GET"]))
            .push(Router::with_path("users/{id}").goal(hello));
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:8698/users/42?sig=secret")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(content, "Hello");
    }

    #[test]
    fn test_tracing_debug() {
        let tracer = NoopTracerProvider::new().tracer("test");
        let handler = Tracing::new(tracer);
        assert!(format!("{handler:?}").contains("Tracing"));
    }
}
