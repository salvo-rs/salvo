//! Request id middleware.
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_extra::request_id::RequestId;
//!
//! #[handler]
//! async fn hello(req: &mut Request) -> String {
//!     format!("Request id: {:?}", req.header::<String>("x-request-id"))
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
//!     let router = Router::new().hoop(RequestId::new()).get(hello);
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
use std::fmt::{self, Debug, Formatter};
use tracing::Instrument;
use ulid::Ulid;

use salvo_core::http::{HeaderValue, Request, Response, header::HeaderName};
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};

/// Key for incoming flash messages in depot.
pub const REQUEST_ID_KEY: &str = "::salvo::request_id";

/// Extension for Depot.
pub trait RequestIdDepotExt {
    /// Get request id reference from depot.
    fn csrf_token(&self) -> Option<&str>;
}

impl RequestIdDepotExt for Depot {
    #[inline]
    fn csrf_token(&self) -> Option<&str> {
        self.get::<String>(REQUEST_ID_KEY).map(|v| &**v).ok()
    }
}

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

impl Debug for RequestId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestId")
            .field("header_name", &self.header_name)
            .field("overwrite", &self.overwrite)
            .finish()
    }
}

impl RequestId {
    /// Create new `CatchPanic` middleware.
    #[must_use]
    pub fn new() -> Self {
        Self {
            header_name: HeaderName::from_static("x-request-id"),
            overwrite: true,
            generator: Box::new(UlidGenerator::new()),
        }
    }

    /// Set the header name for request id.
    #[must_use]
    pub fn header_name(mut self, name: HeaderName) -> Self {
        self.header_name = name;
        self
    }

    /// Set whether overwrite exists request id. Default is `true`.
    #[must_use]
    pub fn overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Set the generator for request id.
    #[must_use]
    pub fn generator(mut self, generator: impl IdGenerator + Send + Sync + 'static) -> Self {
        self.generator = Box::new(generator);
        self
    }

    fn generate_id(&self, req: &mut Request, depot: &mut Depot) -> HeaderValue {
        let id = self.generator.generate(req, depot);
        HeaderValue::from_str(&id).expect("invalid header value")
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
pub struct UlidGenerator {}
impl UlidGenerator {
    /// Create new `UlidGenerator`.
    #[must_use]
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
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let request_id = match req.headers().get(&self.header_name) {
            None => self.generate_id(req, depot),
            Some(value) => {
                if self.overwrite {
                    self.generate_id(req, depot)
                } else {
                    value.clone()
                }
            }
        };

        let _ = req.add_header(self.header_name.clone(), &request_id, false);

        let span = tracing::info_span!("request", ?request_id);

        res.headers_mut()
            .insert(self.header_name.clone(), request_id.clone());

        depot.insert(REQUEST_ID_KEY, request_id);


        async move {
            ctrl.call_next(req, depot, res).await;
        }
            .instrument(span)
            .await;
    }
}
#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::{TestClient, ResponseExt};

    use super::*;

    #[tokio::test]
    async fn test_request_id_added() {
        let handler = RequestId::new();
        let router = Router::new().hoop(handler).get(endpoint);
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:5800/").send(&service).await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert!(response.headers.contains_key("x-request-id"));
    }

    #[tokio::test]
    async fn test_request_id_overwrite() {
        let handler = RequestId::new().overwrite(true);
        let router = Router::new().hoop(handler).get(endpoint);
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:5800/")
            .add_header("x-request-id", "existing-id", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_ne!(response.headers.get("x-request-id").unwrap(), "existing-id");
    }

    #[tokio::test]
    async fn test_request_id_no_overwrite() {
        let handler = RequestId::new().overwrite(false);
        let router = Router::new().hoop(handler).get(endpoint);
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:5800/")
            .add_header("x-request-id", "existing-id", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.headers.get("x-request-id").unwrap(), "existing-id");
    }

    #[tokio::test]
    async fn test_custom_generator() {
        let handler = RequestId::new().generator(|| "custom-id".to_string());
        let router = Router::new().hoop(handler).get(endpoint);
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:5800/").send(&service).await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.headers.get("x-request-id").unwrap(), "custom-id");
    }

    #[tokio::test]
    async fn test_depot_storage() {
        let handler = RequestId::new();
        #[handler]
        async fn depot_checker(depot: &mut Depot, res: &mut Response) {
            let id = depot.get::<HeaderValue>(REQUEST_ID_KEY).unwrap().clone();
            res.render(Text::Plain(id.to_str().unwrap().to_string()));
        }
        let router = Router::new().hoop(handler).get(depot_checker);
        let service = Service::new(router);

        let mut response = TestClient::get("http://127.0.0.1:5800/").send(&service).await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        let header_id = response.headers.get("x-request-id").unwrap().to_str().unwrap().to_string();
        let body = response.take_string().await.unwrap();
        assert_eq!(header_id, body);
    }

    #[handler]
    async fn endpoint() {
    }
}