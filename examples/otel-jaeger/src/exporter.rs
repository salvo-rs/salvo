use std::collections::HashMap;

use opentelemetry::sdk::{
    export::metrics::aggregation,
    metrics::{controllers, controllers::BasicController, processors, selectors},
};
use prometheus::{Encoder, Registry, TextEncoder};

use salvo::http::{Method, Request, Response, StatusCode};

pub struct ExporterHandler(PrometheusExporter);
#[handler]
impl ExporterHandler {
    pub fn new(exporter: PrometheusExporter) -> Self {
        Self(exporter)
    }
    fn handle(&self, req: Request, res: &mut Response) {
        if req.method() != Method::GET {
            return StatusCode::METHOD_NOT_ALLOWED.into();
        }

        let encoder = TextEncoder::new();
        let metric_families = self.0.registry().gather();
        let mut body = Vec::new();
        match encoder.encode(&metric_families, &mut body) {
            Ok(()) => {
                res.stuff(TEXT_PLAIN, body);
            }
            Err(_) => {
                res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }
}
