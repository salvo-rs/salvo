use std::time::Instant;

use opentelemetry::metrics::{Histogram, UpDownCounter};
use opentelemetry::{InstrumentationScope, KeyValue, global};
use opentelemetry_semantic_conventions::{SCHEMA_URL, attribute, metric};
use salvo_core::http::headers::{self, HeaderMapExt};
use salvo_core::prelude::*;

use crate::semconv::{self, KnownMethods, OTHER};

/// Bucket boundaries, in seconds, the HTTP semantic conventions recommend for
/// `http.server.request.duration`.
const DURATION_BOUNDARIES: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0,
];

/// Middleware recording the HTTP server metrics defined by the OpenTelemetry
/// [HTTP semantic conventions].
///
/// # Instruments
///
/// | Name | Instrument | Unit |
/// |------|------------|------|
/// | `http.server.request.duration` | histogram | `s` |
/// | `http.server.active_requests` | up-down counter | `{request}` |
/// | `http.server.request.body.size` | histogram | `By` |
/// | `http.server.response.body.size` | histogram | `By` |
///
/// There is deliberately no request or error counter: the duration histogram
/// already carries the request count (a Prometheus exporter renders it as
/// `http_server_request_duration_seconds_count`), and failures are selected by
/// filtering on `error.type` or `http.response.status_code`.
///
/// # Attributes
///
/// `http.server.active_requests` carries `http.request.method` and `url.scheme`.
/// The other three carry, additionally, `network.protocol.version`,
/// `http.response.status_code`, `http.route` when a route matched, and
/// `error.type` when the response status is a server error.
///
/// Every attribute is drawn from a bounded set, so the number of time series
/// stays proportional to the number of routes rather than growing with traffic.
/// In particular the request target is reported as the matched route template
/// (`/users/{id}`) rather than the requested URI, and the two values a client
/// can otherwise choose — the request method and, through an absolute-form
/// request target, the scheme — collapse to `_OTHER` when they fall outside the
/// known set. See [`Metrics::with_known_methods`].
///
/// [HTTP semantic conventions]: https://opentelemetry.io/docs/specs/semconv/http/http-metrics/
///
/// # Route attribute
///
/// `http.route` requires the `matched-path` feature, which is enabled by
/// default. Through the `salvo` crate, enable its own `matched-path` feature
/// (also on by default) to turn this one on.
///
/// A request that matched no route carries no `http.route`, as the conventions
/// ask, rather than one built from the path it asked for. The exception is a
/// request for `/`, which is reported as the root route: salvo reports a goal
/// mounted at the router root and a request that matched nothing with the same
/// empty matched path, and an application that has no root route has no root
/// traffic for its `/` misses to be confused with.
///
/// # Body sizes
///
/// The body size histograms only record a measurement when the size is known
/// before the response is written: `http.server.request.body.size` comes from
/// the request's `Content-Length`, and `http.server.response.body.size` from a
/// buffered response body. Streamed bodies, `HEAD` responses and error
/// responses whose body the catcher fills in after middleware returns are
/// skipped rather than recorded as zero.
///
/// # Example
///
/// ```ignore
/// use salvo_core::prelude::*;
/// use salvo_otel::Metrics;
///
/// let router = Router::new().hoop(Metrics::new()).get(handler);
/// ```
#[derive(Debug)]
pub struct Metrics {
    duration: Histogram<f64>,
    active_requests: UpDownCounter<i64>,
    request_body_size: Histogram<u64>,
    response_body_size: Histogram<u64>,
    known_methods: Option<KnownMethods>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// Creates `Metrics` middleware, registering its instruments on the global
    /// meter provider.
    ///
    /// The instruments bind to whichever provider is installed at this moment,
    /// so call [`global::set_meter_provider`] first. Built before the
    /// application's provider is installed, the middleware binds to the no-op
    /// provider and records nothing.
    #[must_use]
    pub fn new() -> Self {
        let scope = InstrumentationScope::builder(env!("CARGO_PKG_NAME"))
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_schema_url(SCHEMA_URL)
            .build();
        let meter = global::meter_with_scope(scope);
        Self {
            duration: meter
                .f64_histogram(metric::HTTP_SERVER_REQUEST_DURATION)
                .with_unit("s")
                .with_description("Duration of HTTP server requests.")
                .with_boundaries(DURATION_BOUNDARIES.to_vec())
                .build(),
            active_requests: meter
                .i64_up_down_counter(metric::HTTP_SERVER_ACTIVE_REQUESTS)
                .with_unit("{request}")
                .with_description("Number of active HTTP server requests.")
                .build(),
            request_body_size: meter
                .u64_histogram(metric::HTTP_SERVER_REQUEST_BODY_SIZE)
                .with_unit("By")
                .with_description("Size of HTTP server request bodies.")
                .build(),
            response_body_size: meter
                .u64_histogram(metric::HTTP_SERVER_RESPONSE_BODY_SIZE)
                .with_unit("By")
                .with_description("Size of HTTP server response bodies.")
                .build(),
            known_methods: None,
        }
    }

    /// Replaces the set of methods reported verbatim in `http.request.method`.
    ///
    /// By default only the methods RFC 9110 defines, plus `PATCH` from RFC 5789,
    /// are reported as themselves; any other method becomes `_OTHER` so a client
    /// cannot create a time series per made-up method name. An application that
    /// serves further methods — WebDAV's `PROPFIND`, say — can list them here.
    ///
    /// The set replaces the default rather than extending it, matching how the
    /// conventions define `OTEL_INSTRUMENTATION_HTTP_KNOWN_METHODS`, so include
    /// every method that should be reported verbatim. Names are matched
    /// case-sensitively.
    ///
    /// ```ignore
    /// use salvo_otel::Metrics;
    ///
    /// let metrics = Metrics::new().with_known_methods(["GET", "POST", "PROPFIND"]);
    /// ```
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

/// Keeps `http.server.active_requests` balanced.
///
/// The decrement happens on drop so it also runs when the request future is
/// cancelled — a client disconnect or a timeout drops the future mid-await, and
/// decrementing only after `call_next` returned would leak the increment and
/// leave the gauge drifting upwards for the life of the process.
struct ActiveRequestGuard<'a> {
    counter: &'a UpDownCounter<i64>,
    attrs: Vec<KeyValue>,
}

impl<'a> ActiveRequestGuard<'a> {
    fn new(counter: &'a UpDownCounter<i64>, attrs: Vec<KeyValue>) -> Self {
        counter.add(1, &attrs);
        Self { counter, attrs }
    }
}

impl Drop for ActiveRequestGuard<'_> {
    fn drop(&mut self) {
        self.counter.add(-1, &self.attrs);
    }
}

#[async_trait]
impl Handler for Metrics {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let method = KeyValue::new(
            attribute::HTTP_REQUEST_METHOD,
            semconv::known_method_value(req.method(), self.known_methods.as_ref())
                .unwrap_or_else(|| OTHER.into()),
        );
        let scheme = KeyValue::new(
            attribute::URL_SCHEME,
            semconv::bounded_scheme_value(req.scheme()),
        );

        // Held for the whole request, including the `call_next` await below.
        let _active =
            ActiveRequestGuard::new(&self.active_requests, vec![method.clone(), scheme.clone()]);

        let mut labels = Vec::with_capacity(6);
        labels.push(method);
        labels.push(scheme);
        if let Some(version) = semconv::protocol_version(req.version()) {
            labels.push(KeyValue::new(attribute::NETWORK_PROTOCOL_VERSION, version));
        }
        // Read before the body is consumed downstream.
        let request_body_size = req
            .headers()
            .typed_get::<headers::ContentLength>()
            .map(|length| length.0);

        let started = Instant::now();
        ctrl.call_next(req, depot, res).await;
        let elapsed = started.elapsed();

        #[cfg(feature = "matched-path")]
        if let Some(route) = semconv::route_value(req) {
            labels.push(KeyValue::new(attribute::HTTP_ROUTE, route));
        }

        let status = res.status_code.unwrap_or_else(|| {
            tracing::info!("[otel::Metrics] Treat status_code=none as 200(OK)");
            StatusCode::OK
        });
        labels.push(KeyValue::new(
            attribute::HTTP_RESPONSE_STATUS_CODE,
            i64::from(status.as_u16()),
        ));
        // Only a server error counts as a failed request, and the conventions
        // use the status code as the low-cardinality class of that error.
        if status.is_server_error() {
            labels.push(KeyValue::new(
                attribute::ERROR_TYPE,
                status.as_str().to_owned(),
            ));
        }

        self.duration.record(elapsed.as_secs_f64(), &labels);
        if let Some(size) = request_body_size {
            self.request_body_size.record(size, &labels);
        }
        if let Some(size) = semconv::final_body_size(req, res, status) {
            self.response_body_size.record(size, &labels);
        }
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[tokio::test]
    async fn test_metrics_default() {
        let metrics = Metrics::default();
        assert!(format!("{metrics:?}").contains("Metrics"));
    }

    #[handler]
    async fn hello() -> &'static str {
        "Hello"
    }

    #[tokio::test]
    async fn test_metrics_handle() {
        let metrics = Metrics::default();

        let router = Router::new().hoop(metrics).goal(hello);
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:8698")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(content, "Hello");
    }

    #[tokio::test]
    async fn test_metrics_handle_error() {
        #[handler]
        async fn boom() -> Result<(), StatusError> {
            Err(StatusError::internal_server_error())
        }

        let router = Router::new().hoop(Metrics::new()).goal(boom);
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1:8698")
            .send(&service)
            .await;
        assert_eq!(res.status_code, Some(StatusCode::INTERNAL_SERVER_ERROR));
    }

    #[tokio::test]
    async fn test_metrics_with_known_methods() {
        let metrics = Metrics::new().with_known_methods(["GET", "PROPFIND"]);
        assert!(metrics.known_methods.is_some());

        let router = Router::new().hoop(metrics).goal(hello);
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:8698")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(content, "Hello");
    }
}
