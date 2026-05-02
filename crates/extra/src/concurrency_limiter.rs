//! Middleware for limiting concurrency.
//! 
//! This middleware limits the maximum number of requests being processed concurrently,
//! which helps prevent server overload during traffic spikes.
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

use tokio::sync::{Semaphore, TryAcquireError};

use salvo_core::http::StatusError;
use salvo_core::http::{Request, Response};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

/// MaxConcurrency
#[derive(Debug)]
pub struct MaxConcurrency {
    semaphore: Semaphore,
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
        match self.semaphore.try_acquire() {
            Ok(_permit) => {
                ctrl.call_next(req, depot, res).await;
            }
            Err(e) => match e {
                TryAcquireError::Closed => {
                    tracing::error!(
                        "Max concurrency semaphore is never closed, acquire should never fail: {}",
                        e
                    );
                    res.render(StatusError::payload_too_large().brief("max concurrency reached"));
                }
                TryAcquireError::NoPermits => {
                    tracing::error!("no permits: {}", e);
                    res.render(StatusError::too_many_requests().brief("max concurrency reached"));
                }
            },
        }
    }
}
/// Create a new `MaxConcurrency`.
#[inline]
#[must_use] pub fn max_concurrency(size: usize) -> MaxConcurrency {
    MaxConcurrency {
        semaphore: Semaphore::new(size),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Notify;

    use salvo_core::http::StatusCode;
    use salvo_core::prelude::*;
    use salvo_core::test::TestClient;

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

        let second = TestClient::get("http://127.0.0.1:5801")
            .send(router)
            .await;
        assert_eq!(second.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        release.notify_one();
        let first = first.await.unwrap();
        assert_eq!(first.status_code, Some(StatusCode::OK));
    }
}
