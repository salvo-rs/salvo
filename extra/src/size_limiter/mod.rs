use async_trait::async_trait;
use salvo_core::http::errors::*;
use salvo_core::http::HttpBody;
use salvo_core::prelude::*;

pub struct MaxSizeHandler(u64);
#[async_trait]
impl Handler for MaxSizeHandler {
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        if let Some(upper) = req.body().and_then(|body| body.size_hint().upper()) {
            if upper > self.0 {
                res.set_http_error(PayloadTooLarge());
            }
        }
    }
}

pub fn max_size(size: u64) -> MaxSizeHandler {
    MaxSizeHandler(size)
}
