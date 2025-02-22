use std::time::Instant;

use opentelemetry::metrics::{Counter, Histogram};
use opentelemetry::{KeyValue, global};
use opentelemetry_semantic_conventions::trace;
use salvo_core::http::ResBody;
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

impl Metrics {
    /// Create `Metrics` middleware with `meter`.
    pub fn new() -> Self {
        let meter = global::meter("salvo");
        Self {
            request_count: meter
                .u64_counter("salvo_request_count")
                .with_description("total request count (since start of service)")
                .build(),
            error_count: meter
                .u64_counter("salvo_error_count")
                .with_description("failed request count (since start of service)")
                .build(),
            duration: meter
                .f64_histogram("salvo_request_duration_ms")
                .with_unit("milliseconds")
                .with_description(
                    "request duration histogram (in milliseconds, since start of service)",
                )
                .build(),
        }
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
        let mut labels = Vec::with_capacity(3);
        labels.push(KeyValue::new(
            trace::HTTP_REQUEST_METHOD,
            req.method().to_string(),
        ));
        labels.push(KeyValue::new(trace::URL_FULL, req.uri().to_string()));

        let s = Instant::now();
        ctrl.call_next(req, depot, res).await;
        let elapsed = s.elapsed();

        let status = res.status_code.unwrap_or_else(|| {
            tracing::info!("[otel::Metrics] Treat status_code=none as 200(OK).");
            StatusCode::OK
        });
        labels.push(KeyValue::new(
            trace::HTTP_RESPONSE_STATUS_CODE,
            status.as_u16() as i64,
        ));
        if status.is_client_error() || status.is_server_error() {
            self.error_count.add(1, &labels);
            let msg = if let ResBody::Error(body) = &res.body {
                body.to_string()
            } else {
                format!("ErrorCode: {}", status.as_u16())
            };
            labels.push(KeyValue::new(trace::EXCEPTION_MESSAGE, msg));
        }

        self.request_count.add(1, &labels);
        self.duration
            .record(elapsed.as_secs_f64() * 1000.0, &labels);
    }
}
