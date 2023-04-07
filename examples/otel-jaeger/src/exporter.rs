use opentelemetry::sdk::export::metrics::aggregation;
use opentelemetry::sdk::metrics::{controllers, processors, selectors};
use opentelemetry_prometheus::PrometheusExporter;
use prometheus::{Encoder, Registry, TextEncoder};

use salvo::http::{header, Method, StatusCode};
use salvo::prelude::*;

pub struct Exporter(PrometheusExporter);
#[handler]
impl Exporter {
    pub fn new() -> Self {
        let controller = controllers::basic(processors::factory(
            selectors::simple::histogram([1.0, 2.0, 5.0, 10.0, 20.0, 50.0]),
            aggregation::cumulative_temporality_selector(),
        ))
        .build();
        let exporter = opentelemetry_prometheus::exporter(controller)
            .with_registry(Registry::new_custom(None, None).expect("create prometheus registry"))
            .init();
        Self(exporter)
    }
    fn handle(&self, req: &mut Request, res: &mut Response) {
        if req.method() != Method::GET {
            res.set_status_code(StatusCode::METHOD_NOT_ALLOWED);
            return;
        }

        let encoder = TextEncoder::new();
        let metric_families = self.0.registry().gather();
        let mut body = Vec::new();
        match encoder.encode(&metric_families, &mut body) {
            Ok(()) => {
                res.add_header(header::CONTENT_TYPE, "text/javascript; charset=utf-8", true)
                    .ok();
                res.set_body(body.into());
            }
            Err(_) => {
                res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }
}
