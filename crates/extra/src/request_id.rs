//! Request id middleware.
//!
//! Read more: <https://salvo.rs>
use ulid::Ulid;

use salvo_core::http::{header::HeaderName, Request, Response};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

/// Key for incoming flash messages in depot.
pub const REQUST_ID_KEY: &str = "::salvo::request_id";

/// A middleware for generate request id.
#[non_exhaustive]
pub struct RequestId {
    /// The header name for request id.
    pub header_name: HeaderName,
    /// Whether overwrite exists request id. Default is `true`
    pub overwrite: bool,
    /// The generator for request id.
    pub generator: Box<dyn IdGenerator + Send + Sync>,
}

impl RequestId {
    /// Create new `CatchPanic` middleware.
    pub fn new() -> Self {
        Self {
            header_name: HeaderName::from_static("x-request-id"),
            overwrite: true,
            generator: Box::new(UlidGenerator::new()),
        }
    }

    /// Set the header name for request id.
    pub fn header_name(mut self, name: HeaderName) -> Self {
        self.header_name = name;
        self
    }

    /// Set whether overwrite exists request id. Default is `true`.
    pub fn overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Set the generator for request id.
    pub fn generator(mut self, generator: impl IdGenerator + Send + Sync + 'static) -> Self {
        self.generator = Box::new(generator);
        self
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

/// A trait for generate request id.
pub trait IdGenerator {
    /// Generate a new request id.
    fn generate(&self, req: &mut Request, depot: &mut Depot) -> String;
}

impl<F> IdGenerator for F
where
    F: Fn() -> String + Send + Sync,
{
    fn generate(&self, _req: &mut Request, _depot: &mut Depot) -> String {
        self()
    }
}

/// A generator for generate request id with ulid.
#[derive(Default, Debug)]
pub struct UlidGenerator{}
impl UlidGenerator{
    /// Create new `UlidGenerator`.
    pub fn new() -> Self {
        Self {}
    }
}
impl IdGenerator for UlidGenerator {
    fn generate(&self, _req: &mut Request, _depot: &mut Depot) -> String {
        Ulid::new().to_string()
    }
}

#[async_trait]
impl Handler for RequestId {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, _res: &mut Response, _ctrl: &mut FlowCtrl) {
        if !self.overwrite && req.headers().contains_key(REQUST_ID_KEY) {
            return;
        }
        let id = self.generator.generate(req, depot);
        req.add_header(self.header_name.clone(), &id, true).ok();
        depot.insert(REQUST_ID_KEY, id);
    }
}
