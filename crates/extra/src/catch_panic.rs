//! Middleware for catch panic in handlers.
//!
//! This middleware catches panics and write `500 Internal Server Error` into response.
//! This middleware should be used as the first middleware.
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_extra::catch_panic::CatchPanic;
//!
//! #[handler]
//! async fn hello() {
//!     panic!("panic error!");
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new().hoop(CatchPanic::new()).get(hello);
//!     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```

use std::panic::AssertUnwindSafe;

use futures_util::FutureExt;

use salvo_core::http::{Request, Response, StatusError};
use salvo_core::{async_trait, Depot, FlowCtrl, Error, Handler};


/// Middleware that catches panics in handlers and converts them to HTTP 500 responses.
/// 
/// This middleware should be registered as the first middleware in your router chain
/// to ensure it catches panics from all subsequent handlers and middlewares.
/// 
/// View [module level documentation](index.html) for more details.
#[derive(Default, Debug)]
pub struct CatchPanic {}
impl CatchPanic {
    /// Create new `CatchPanic` middleware.
    #[inline]
    pub fn new() -> Self {
        CatchPanic {}
    }
}

#[async_trait]
impl Handler for CatchPanic {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if let Err(e) = AssertUnwindSafe(ctrl.call_next(req, depot, res)).catch_unwind().await {
            tracing::error!(error = ?e, "panic occurred");
            res.render(
                StatusError::internal_server_error()
                    .brief("Panic occurred on server.")
                    .cause(Error::other(format!("{e:#?}"))),
            );
        }
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
    async fn test_catch_panic() {
        #[handler]
        async fn hello() -> &'static str {
            panic!("panic error!");
        }

        let router = Router::new()
            .hoop(CatchPanic::new())
            .push(Router::with_path("hello").get(hello));

        TestClient::get("http://127.0.0.1:5801/hello")
            .send(router)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(logs_contain("panic occurred"));
    }
}
