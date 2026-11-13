//! Deadline for downstream handler processing.
//!
//! If `FlowCtrl::call_next` does not finish within the configured duration, this middleware
//! drops that future and produces a `503 Service Unavailable` response. The deadline covers
//! request-body reads performed by those handlers and an upstream proxy's wait for response
//! headers. It ends when the handlers return, even if the response body is still streaming.
//! Catchers or middleware outside this call are outside its deadline. Cancellation does not
//! undo completed side effects, stop detached tasks, or preempt blocking synchronous code.
//!
//! # Choose timeouts by phase
//!
//! | Phase | Configuration | Scope |
//! |-------|---------------|-------|
//! | Downstream handler work | [`Timeout`] | One `call_next` future |
//! | TLS handshake / incoming HTTP/1 headers | [`FuseConfig`](salvo_core::fuse::FuseConfig) | Accepted connection; HTTP/1 detection and headers share one budget |
//! | Incoming request-body inactivity | `FuseConfig::request_body_timeout` | Gap between request-body frames |
//! | Transport inactivity / blocked writes | `FuseConfig::connection_idle_timeout` / `write_stall_timeout` | TCP-based connection; multiplexed streams share transport activity |
//! | Upstream connect / reads / total request | Configure the proxy's HTTP client | Separate from the inbound server fuse and this middleware |
//!
//! A handler deadline does not enforce an outgoing response-body idle timeout or a total
//! streaming lifetime. `ResBody::observe` measures body production, errors, and early drops;
//! it does not enforce a timeout or prove socket delivery. Bound custom producers explicitly
//! when a response-body deadline is required. Once headers have been sent, a stream failure
//! cannot be replaced with a fresh `503` response.
//!
//! For SSE and other intentional long streams, keep the handler's setup deadline separate
//! from stream lifetime. Configure heartbeats and any upstream read timeout for the expected
//! quiet interval; a short total upstream deadline will terminate a healthy long stream.
//! TCP idle/write limits can close legitimate idle or slow connections and do not provide
//! per-response inactivity isolation on HTTP/2. HTTP/3 uses QUIC transport limits instead.
//!
//! # Example
//!
//! This finite-response service uses a five-second handler deadline and separately enables
//! inbound body-frame, transport idle, and write-stall limits. Choose values for the workload;
//! these transport limits need reconsideration before using the same listener for SSE.
//!
//! ```no_run
//! use std::time::Duration;
//!
//! use salvo_core::prelude::*;
//! use salvo_core::fuse::FuseConfig;
//! use salvo_extra::timeout::Timeout;
//!
//! #[handler]
//! async fn fast() -> &'static str {
//!     "hello"
//! }
//! #[handler]
//! async fn slow() -> &'static str {
//!     tokio::time::sleep(Duration::from_secs(6)).await;
//!     "hello"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new()
//!         .hoop(Timeout::new(Duration::from_secs(5)))
//!         .push(Router::with_path("slow").get(slow))
//!         .push(Router::with_path("fast").get(fast));
//!
//!     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
//!     let mut fuse = FuseConfig::default();
//!     fuse.request_body_timeout = Some(Duration::from_secs(15));
//!     fuse.connection_idle_timeout = Some(Duration::from_secs(60));
//!     fuse.write_stall_timeout = Some(Duration::from_secs(15));
//!     Server::new(acceptor).fuse_config(fuse).serve(router).await;
//! }
//! ```
use std::fmt::{self, Debug, Formatter};
use std::time::Duration;

use salvo_core::http::headers::{Connection, HeaderMapExt};
use salvo_core::http::{Request, Response, StatusError};
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};

/// Deadline for downstream handler processing, ending before response-body transmission.
///
/// View [module level documentation](index.html) for more details.
pub struct Timeout {
    value: Duration,
    error: Box<dyn Fn() -> StatusError + Send + Sync + 'static>,
}

impl Debug for Timeout {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Timeout")
            .field("value", &self.value)
            .finish()
    }
}

impl Timeout {
    /// Create a new `Timeout`.
    #[inline]
    #[must_use]
    pub fn new(value: Duration) -> Self {
        // If a 408 error code is returned, the browser may resend the request multiple times. In most cases,
        // this behavior is undesirable.
        // https://github.com/tower-rs/tower-http/issues/300
        Self {
            value,
            error: Box::new(|| {
                StatusError::service_unavailable()
                    .brief("server timed out while processing the request")
            }),
        }
    }

    /// Custom error returned when timeout.
    ///
    /// By default, a `503 Service Unavailable` error is returned. You can set this function to other error types,
    /// such as `408 Request Timeout`, but the 408 error code may cause the browser to automatically resend the
    /// request multiple times.
    #[must_use]
    pub fn error(mut self, error: impl Fn() -> StatusError + Send + Sync + 'static) -> Self {
        self.error = Box::new(error);
        self
    }
}
#[async_trait]
impl Handler for Timeout {
    #[inline]
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        tokio::select! {
            _ = ctrl.call_next(req, depot, res) => {},
            _ = tokio::time::sleep(self.value) => {
                res.headers_mut().typed_insert(Connection::close());
                res.render((self.error)());
                ctrl.skip_rest();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[tokio::test]
    async fn replacement_error_discards_previous_encoding_and_length() {
        #[handler]
        async fn slow(res: &mut Response) {
            res.add_header("content-encoding", "gzip", true).unwrap();
            res.add_header("content-length", "1", true).unwrap();
            res.add_header("etag", "old", true).unwrap();
            res.render("x");
            std::future::pending::<()>().await;
        }
        let service = Service::new(
            Router::new()
                .hoop(Timeout::new(Duration::from_millis(10)))
                .get(slow),
        );
        let mut response = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(response.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
        assert!(response.headers().get("content-encoding").is_none());
        assert!(response.headers().get("content-length").is_none());
        assert!(response.headers().get("etag").is_none());
        assert!(response.take_string().await.unwrap().contains("timed out"));
    }

    #[tokio::test]
    async fn test_timeout_handler() {
        #[handler]
        async fn fast() -> &'static str {
            "hello"
        }
        #[handler]
        async fn slow() -> &'static str {
            tokio::time::sleep(Duration::from_secs(6)).await;
            "hello"
        }

        let router = Router::new()
            .hoop(Timeout::new(Duration::from_secs(5)))
            .push(Router::with_path("slow").get(slow))
            .push(Router::with_path("fast").get(fast));
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:5801/slow")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("timed out while processing the request"));

        let content = TestClient::get("http://127.0.0.1:5801/fast")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("hello"));
    }
}
