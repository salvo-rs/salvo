//! Middleware for limiting concurrency.
//!
//! This middleware limits the maximum number of requests being processed concurrently,
//! which helps prevent server overload during traffic spikes.
//! The default budget ends with handler execution. Opt into [`MaxConcurrency::include_body`]
//! to retain it through downstream body production; this is separate from socket delivery
//! and upgraded-protocol session limits.
//!
//! # Example
//!
//! ```no_run
//! use std::fs::create_dir_all;
//! use std::path::Path;
//!
//! use salvo_core::prelude::*;
//! use salvo_extra::concurrency_limiter::*;
//!
//! #[handler]
//! async fn index(res: &mut Response) {
//!     res.render(Text::Html(INDEX_HTML));
//! }
//! #[handler]
//! async fn upload(req: &mut Request, res: &mut Response) {
//!     let file = req.file("file").await;
//!     tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
//!     if let Some(file) = file {
//!         let dest = format!("temp/{}", file.name().unwrap_or("file"));
//!         tracing::debug!(dest = %dest, "upload file");
//!         if let Err(e) = std::fs::copy(file.path(), Path::new(&dest)) {
//!             res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
//!             res.render(Text::Plain(format!("file not found in request: {e}")));
//!         } else {
//!             res.render(Text::Plain(format!("File uploaded to {dest}")));
//!         }
//!     } else {
//!         res.status_code(StatusCode::BAD_REQUEST);
//!         res.render(Text::Plain("file not found in request"));
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     create_dir_all("temp").unwrap();
//!     let router = Router::new()
//!         .get(index)
//!         .push(Router::new().hoop(max_concurrency(1)).path("limited").post(upload))
//!         .push(Router::with_path("unlimit").post(upload));
//!
//!     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//!
//! static INDEX_HTML: &str = r#"<!DOCTYPE html>
//! <html>
//!     <head>
//!         <title>Upload file</title>
//!     </head>
//!     <body>
//!         <h1>Upload file</h1>
//!         <form action="/unlimit" method="post" enctype="multipart/form-data">
//!             <h3>Unlimit</h3>
//!             <input type="file" name="file" />
//!             <input type="submit" value="upload" />
//!         </form>
//!         <form action="/limited" method="post" enctype="multipart/form-data">
//!             <h3>Limited</h3>
//!             <input type="file" name="file" />
//!             <input type="submit" value="upload" />
//!         </form>
//!     </body>
//! </html>
//! "#;
//! ```

use std::sync::Arc;

use salvo_core::http::{Request, Response, StatusError};
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};
use tokio::sync::{Semaphore, TryAcquireError};

/// MaxConcurrency
#[derive(Debug)]
pub struct MaxConcurrency {
    semaphore: Arc<Semaphore>,
    include_body: bool,
}

impl MaxConcurrency {
    /// Also retain capacity while the response body is being produced.
    ///
    /// By default capacity covers only downstream handler execution. With this option,
    /// the permit is released when body production ends, errors, or is dropped (including
    /// HEAD suppression). It does not cover socket acknowledgement or upgraded protocols.
    /// Place this outside body-producing middleware. Use Server::max_connections for
    /// physical connections and application-owned budgets for upgraded/background work.
    /// Capacity follows the downstream body; later response replacement releases that
    /// body's permit and does not transfer it to a replacement produced by a finalizer.
    ///
    /// ```
    /// use salvo_extra::concurrency_limiter::max_concurrency;
    /// let budget = max_concurrency(32).include_body(true);
    /// ```
    #[must_use]
    pub fn include_body(mut self, include_body: bool) -> Self {
        self.include_body = include_body;
        self
    }
}
#[async_trait]
impl Handler for MaxConcurrency {
    #[inline]
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        if !self.include_body {
            match self.semaphore.try_acquire() {
                Ok(_permit) => {
                    ctrl.call_next(req, depot, res).await;
                }
                Err(error) => reject(error, res),
            }
            return;
        }
        match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => {
                ctrl.call_next(req, depot, res).await;
                let body = res.take_body();
                res.body(body.observe(move |_| drop(permit)));
            }
            Err(error) => reject(error, res),
        }
    }
}

fn reject(error: TryAcquireError, res: &mut Response) {
    match error {
        TryAcquireError::Closed => {
            tracing::error!(
                "Max concurrency semaphore is never closed, acquire should never fail: {}",
                error
            );
            res.render(StatusError::service_unavailable().brief("max concurrency reached"));
        }
        TryAcquireError::NoPermits => {
            tracing::debug!("max concurrency reached, rejecting request");
            res.render(StatusError::too_many_requests().brief("max concurrency reached"));
        }
    }
}
/// Create a new `MaxConcurrency`.
#[inline]
#[must_use]
pub fn max_concurrency(size: usize) -> MaxConcurrency {
    MaxConcurrency {
        semaphore: Arc::new(Semaphore::new(size)),
        include_body: false,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use salvo_core::http::StatusCode;
    use salvo_core::prelude::*;
    use salvo_core::test::TestClient;
    use tokio::sync::Notify;

    use super::*;

    #[derive(Debug)]
    struct BlockingHandler {
        started: Arc<Notify>,
        release: Arc<Notify>,
    }

    #[async_trait]
    impl Handler for BlockingHandler {
        async fn handle(
            &self,
            _req: &mut Request,
            _depot: &mut Depot,
            res: &mut Response,
            _ctrl: &mut FlowCtrl,
        ) {
            self.started.notify_one();
            self.release.notified().await;
            res.render("done");
        }
    }

    #[tokio::test]
    async fn max_concurrency_holds_permit_until_next_handler_finishes() {
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let router = Arc::new(Router::new().hoop(max_concurrency(1)).get(BlockingHandler {
            started: started.clone(),
            release: release.clone(),
        }));

        let first_router = router.clone();
        let first = tokio::spawn(async move {
            TestClient::get("http://127.0.0.1:5801")
                .send(first_router)
                .await
        });
        started.notified().await;

        let second = TestClient::get("http://127.0.0.1:5801").send(router).await;
        assert_eq!(second.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        release.notify_one();
        let first = first.await.unwrap();
        assert_eq!(first.status_code, Some(StatusCode::OK));
    }

    #[salvo_core::handler]
    async fn buffered_body(res: &mut Response) {
        res.render("body");
    }

    #[tokio::test]
    async fn optional_body_budget_is_released_by_consumption_or_drop() {
        use salvo_core::test::ResponseExt;
        let service = Service::new(
            Router::new()
                .hoop(max_concurrency(1).include_body(true))
                .goal(buffered_body),
        );
        let mut first = TestClient::get("http://localhost/").send(&service).await;
        let blocked = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(blocked.status_code, Some(StatusCode::TOO_MANY_REQUESTS));
        assert_eq!(first.take_bytes(None).await.unwrap(), "body");
        let second = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(second.status_code, Some(StatusCode::OK));
        drop(second);
        let third = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(third.status_code, Some(StatusCode::OK));
    }

    #[tokio::test]
    async fn default_budget_remains_handler_only_and_head_releases_the_body() {
        let default = Service::new(Router::new().hoop(max_concurrency(1)).goal(buffered_body));
        let first = TestClient::get("http://localhost/").send(&default).await;
        let second = TestClient::get("http://localhost/").send(&default).await;
        assert_eq!(first.status_code, Some(StatusCode::OK));
        assert_eq!(second.status_code, Some(StatusCode::OK));
        let body_budget = Service::new(
            Router::new()
                .hoop(max_concurrency(1).include_body(true))
                .goal(buffered_body),
        );
        let head = TestClient::head("http://localhost/")
            .send(&body_budget)
            .await;
        assert!(head.body.is_none());
        let get = TestClient::get("http://localhost/")
            .send(&body_budget)
            .await;
        assert_eq!(get.status_code, Some(StatusCode::OK));
    }

    struct SourceFailure;
    impl salvo_core::http::body::Body for SourceFailure {
        type Data = <salvo_core::http::ResBody as salvo_core::http::body::Body>::Data;
        type Error = salvo_core::BoxedError;
        fn poll_frame(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Result<salvo_core::http::body::Frame<Self::Data>, Self::Error>>>
        {
            std::task::Poll::Ready(Some(Err(std::io::Error::other("source failed").into())))
        }
    }

    #[salvo_core::handler]
    async fn failing_body(res: &mut Response) {
        res.body(salvo_core::http::ResBody::Boxed(Box::pin(SourceFailure)));
    }

    #[tokio::test]
    async fn body_source_failure_releases_capacity() {
        use salvo_core::test::ResponseExt;
        let service = Service::new(
            Router::new()
                .hoop(max_concurrency(1).include_body(true))
                .goal(failing_body),
        );
        let mut first = TestClient::get("http://localhost/").send(&service).await;
        assert!(first.take_bytes(None).await.is_err());
        let next = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(next.status_code, Some(StatusCode::OK));
    }
}
