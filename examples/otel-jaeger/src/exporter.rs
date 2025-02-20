use prometheus::{Encoder, Registry, TextEncoder};

use salvo::http::{Method, StatusCode, header};
use salvo::prelude::*;

pub struct Exporter {
    registry: Registry,
}
#[handler]
impl Exporter {
    pub fn new() -> Self {
        let registry = Registry::new_custom(None, None).expect("create prometheus registry");
        Self { registry }
    }
    fn handle(&self, req: &Request, res: &mut Response) {
        if req.method() != Method::GET {
            res.status_code(StatusCode::METHOD_NOT_ALLOWED);
            return;
        }

        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut body = Vec::new();
        match encoder.encode(&metric_families, &mut body) {
            Ok(()) => {
                let _ =
                    res.add_header(header::CONTENT_TYPE, "text/javascript; charset=utf-8", true);
                res.body(body);
            }
            Err(_) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }
}
