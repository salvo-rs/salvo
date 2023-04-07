use opentelemetry::trace::{FutureExt, Span, SpanKind, TraceContextExt, Tracer};
use opentelemetry::{global, Context};
use opentelemetry_http::HeaderExtractor;
use opentelemetry_semantic_conventions::{resource, trace};
use salvo_core::http::headers::{self, HeaderMapExt};
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
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        let remote_addr = req.remote_addr().to_string();

        let parent_cx =
            global::get_text_map_propagator(|propagator| propagator.extract(&HeaderExtractor(req.headers())));

        let mut attributes = Vec::new();
        attributes.push(resource::TELEMETRY_SDK_NAME.string(env!("CARGO_CRATE_NAME")));
        attributes.push(resource::TELEMETRY_SDK_VERSION.string(env!("CARGO_PKG_VERSION")));
        attributes.push(resource::TELEMETRY_SDK_LANGUAGE.string("rust"));
        attributes.push(trace::HTTP_METHOD.string(req.method().to_string()));
        attributes.push(trace::HTTP_URL.string(req.uri().to_string()));
        attributes.push(trace::HTTP_CLIENT_IP.string(remote_addr));
        attributes.push(trace::HTTP_FLAVOR.string(format!("{:?}", req.version())));
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

            let status = res.status_code().unwrap_or(StatusCode::NOT_FOUND);
            let event = if status.is_client_error() || status.is_server_error() {
                "request.failure"
            } else {
                "request.success"
            };
            span.add_event(event.to_string(), vec![]);
            span.set_attribute(trace::HTTP_STATUS_CODE.i64(status.as_u16() as i64));
            if let Some(content_length) = res.headers().typed_get::<headers::ContentLength>() {
                span.set_attribute(trace::HTTP_RESPONSE_CONTENT_LENGTH.i64(content_length.0 as i64));
            }
        }
        .with_context(Context::current_with_span(span))
        .await
    }
}
