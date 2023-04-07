use std::time::Instant;

use opentelemetry::metrics::{Counter, Histogram, Unit};
use opentelemetry::{global, Context};
use opentelemetry_semantic_conventions::trace;
use salvo_core::prelude::*;

/// Middleware for metrics with OpenTelemetry.
pub struct Metrics {
    request_count: Counter<u64>,
    error_count: Counter<u64>,
    duration: Histogram<f64>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[handler]
impl Metrics {
    /// Create `Metrics` middleware with `meter`.
    pub fn new() -> Self {
        let meter = global::meter("salvo");
        Self {
            request_count: meter
                .u64_counter("salvo_request_count")
                .with_description("total request count (since start of service)")
                .init(),
            error_count: meter
                .u64_counter("salvo_error_count")
                .with_description("failed request count (since start of service)")
                .init(),
            duration: meter
                .f64_histogram("salvo_request_duration_ms")
                .with_unit(Unit::new("milliseconds"))
                .with_description("request duration histogram (in milliseconds, since start of service)")
                .init(),
        }
    }
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        let cx = Context::new();

        let mut labels = Vec::with_capacity(3);
        labels.push(trace::HTTP_METHOD.string(req.method().to_string()));
        labels.push(trace::HTTP_URL.string(req.uri().to_string()));

        let s = Instant::now();
        ctrl.call_next(req, depot, res).await;
        let elapsed = s.elapsed();

        let status = res.status_code().unwrap_or(StatusCode::NOT_FOUND);
        labels.push(trace::HTTP_STATUS_CODE.i64(status.as_u16() as i64));
        if status.is_client_error() || status.is_server_error() {
            self.error_count.add(&cx, 1, &labels);
            let msg = if let Some(e) = res.status_error() {
                e.to_string()
            } else {
                format!("ErrorCode: {}", status.as_u16())
            };
            labels.push(trace::EXCEPTION_MESSAGE.string(msg));
        }

        self.request_count.add(&cx, 1, &labels);
        self.duration.record(&cx, elapsed.as_secs_f64() * 1000.0, &labels);
    }
}
