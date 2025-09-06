//! A simple logging middleware.
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_extra::logging::Logger;
//!
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello World"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new().get(hello);
//!     let service = Service::new(router).hoop(Logger::new());
//!
//!     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
//!     Server::new(acceptor).serve(service).await;
//! }
//! ```
use std::time::Instant;

use tracing::{Instrument, Level};

use salvo_core::http::{Request, ResBody, Response, StatusCode};
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};

/// A simple logger middleware.
#[derive(Default, Debug)]
pub struct Logger {
    /// Whether to log status error.
    ///
    /// If true, the logger will try log [`StatusError`][salvo_core::http::StatusError] information if response body is [`ResBody::Error`].
    ///
    /// **Note**: If you have handled the error before logging and the body is not [`ResBody::Error`],
    /// the error information cannot be recorded.
    pub log_status_error: bool,
}
impl Logger {
    /// Create new `Logger` middleware.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            log_status_error: true,
        }
    }

    /// Set whether to log [`StatusError`][salvo_core::http::StatusError] information if response body is [`ResBody::Error`].
    ///
    /// **Note**: If you have handled the error before logging and the body is not [`ResBody::Error`],
    /// the error information cannot be recorded.
     #[must_use]
    pub fn log_status_error(mut self, log_status_error: bool) -> Self {
        self.log_status_error = log_status_error;
        self
    }
}

#[async_trait]
impl Handler for Logger {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let span = tracing::span!(
            Level::INFO,
            "Request",
            remote_addr = %req.remote_addr().to_string(),
            version = ?req.version(),
            method = %req.method(),
            path = %req.uri(),
        );

        async move {
            let now = Instant::now();
            ctrl.call_next(req, depot, res).await;
            let duration = now.elapsed();

            let status = res.status_code.unwrap_or(match &res.body {
                ResBody::None => StatusCode::NOT_FOUND,
                ResBody::Error(e) => e.code,
                _ => StatusCode::OK,
            });
            if let ResBody::Error(error) = &res.body {
                tracing::info!(
                    %status,
                    ?duration,
                    ?error,
                    "Response"
                );
            } else {
                tracing::info!(
                    %status,
                    ?duration,
                    "Response"
                );
            }
        }
        .instrument(span)
        .await
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};
    use tracing_test::traced_test;

    use super::*;

    #[tokio::test]
    #[traced_test]
    async fn test_log() {
        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .hoop(Logger::new())
            .push(Router::with_path("hello").get(hello));

        TestClient::get("http://127.0.0.1:5801/hello")
            .send(router)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(logs_contain("duration"));
    }
}
