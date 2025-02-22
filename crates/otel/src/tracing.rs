use opentelemetry::trace::{FutureExt, Span, SpanKind, TraceContextExt, Tracer};
use opentelemetry::{Context, KeyValue, global};
use opentelemetry_http::HeaderExtractor;
use opentelemetry_semantic_conventions::{resource, trace};
use salvo_core::http::headers::{self, HeaderMap, HeaderMapExt, HeaderName, HeaderValue};
use salvo_core::prelude::*;

/// Middleware for tracing with OpenTelemetry.
pub struct Tracing<T> {
    tracer: T,
}

impl<T> Tracing<T> {
    /// Create `Tracing` middleware with `tracer`.
    pub fn new(tracer: T) -> Self {
        Self { tracer }
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
        let remote_addr = req.remote_addr().to_string();

        //TODO: Will remove after opentelemetry_http updated
        let mut headers = HeaderMap::with_capacity(req.headers().len());
        headers.extend(req.headers().into_iter().map(|(name, value)| {
            let name = HeaderName::from_bytes(name.as_ref()).expect("Invalid header name");
            let value = HeaderValue::from_bytes(value.as_ref()).expect("Invalid header value");
            (name, value)
        }));

        let parent_cx = global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor(&headers))
        });

        let mut attributes = Vec::new();
        attributes.push(KeyValue::new(
            resource::TELEMETRY_SDK_NAME,
            env!("CARGO_CRATE_NAME"),
        ));
        attributes.push(KeyValue::new(
            resource::TELEMETRY_SDK_VERSION,
            env!("CARGO_PKG_VERSION"),
        ));
        attributes.push(KeyValue::new(resource::TELEMETRY_SDK_LANGUAGE, "rust"));
        attributes.push(KeyValue::new(
            trace::HTTP_REQUEST_METHOD,
            req.method().to_string(),
        ));
        attributes.push(KeyValue::new(trace::URL_FULL, req.uri().to_string()));
        attributes.push(KeyValue::new(trace::CLIENT_ADDRESS, remote_addr));
        attributes.push(KeyValue::new(
            trace::NETWORK_PROTOCOL_VERSION,
            format!("{:?}", req.version()),
        ));
        let mut span = self
            .tracer
            .span_builder(format!("{} {}", req.method(), req.uri()))
            .with_kind(SpanKind::Server)
            .with_attributes(attributes)
            .start_with_context(&self.tracer, &parent_cx);

        span.add_event("request.started".to_string(), vec![]);

        async move {
            ctrl.call_next(req, depot, res).await;
            let cx = Context::current();
            let span = cx.span();

            let status = res.status_code.unwrap_or_else(|| {
                tracing::info!("[otel::Tracing] Treat status_code=none as 200(OK).");
                StatusCode::OK
            });
            let event = if status.is_client_error() || status.is_server_error() {
                "request.failure"
            } else {
                "request.success"
            };
            span.add_event(event.to_string(), vec![]);
            span.set_attribute(KeyValue::new(
                trace::HTTP_RESPONSE_STATUS_CODE,
                status.as_u16() as i64,
            ));
            if let Some(content_length) = res.headers().typed_get::<headers::ContentLength>() {
                span.set_attribute(KeyValue::new(
                    "http.response.header.content-length",
                    content_length.0 as i64,
                ));
            }
        }
        .with_context(Context::current_with_span(span))
        .await
    }
}
