//! Request id middleware
use ulid::Ulid;

use salvo_core::http::{Request, Response};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

/// Key for incoming flash messages in depot.
pub const REQUST_ID_KEY: &str = "::salvo::request_id";

/// A wrapper around [`ulid::Ulid`]
#[derive(Debug)]
pub struct RequestId{}

impl RequestId {
    /// Create new `CatchPanic` middleware.
    pub fn new() -> Self {
        Self {}
    }
}

/// A trait for `Depot` to get request id.
pub trait RequestIdDepotExt {
    /// Get request id.
    fn request_id(&mut self) -> Option<&String>;
}

impl RequestIdDepotExt for Depot {
    #[inline]
    fn request_id(&mut self) -> Option<&String> {
        self.get::<String>(REQUST_ID_KEY).ok()
    }
}

#[async_trait]
impl Handler for RequestId {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, _res: &mut Response, _ctrl: &mut FlowCtrl) {
        let id = Ulid::new().to_string();
        req.add_header("x-request-id", &id, true).ok();
        depot.insert(REQUST_ID_KEY, id);
    }
}