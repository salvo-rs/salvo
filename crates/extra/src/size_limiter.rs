//! Middleware for limiting request size.
//!
//! # Example
//!
//! ```no_run
//! use std::fs::create_dir_all;
//! use std::path::Path;
//! 
//! use salvo_core::prelude::*;
//! use salvo_extra::size_limiter::max_size;
//! 
//! #[handler]
//! async fn index(res: &mut Response) {
//!     res.render(Text::Html(INDEX_HTML));
//! }
//! #[handler]
//! async fn upload(req: &mut Request, res: &mut Response) {
//!     let file = req.file("file").await;
//!     if let Some(file) = file {
//!         let dest = format!("temp/{}", file.name().unwrap_or("file"));
//!         tracing::debug!(dest, "upload file");
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
//!         .push(
//!             Router::new()
//!                 .hoop(max_size(1024 * 1024 * 10))
//!                 .path("limited")
//!                 .post(upload),
//!         )
//!         .push(Router::with_path("unlimit").post(upload));
//! 
//!     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
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
//!             <h3>Limited 10MiB</h3>
//!             <input type="file" name="file" />
//!             <input type="submit" value="upload" />
//!         </form>
//!     </body>
//! </html>
//! "#;
//! ```
use salvo_core::http::StatusError;
use salvo_core::http::{Body, Request, Response};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

/// MaxSize limit for request size.
pub struct MaxSize(pub u64);
#[async_trait]
impl Handler for MaxSize {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        let size_hint = req.body().size_hint().upper();
        if let Some(upper) = size_hint {
            if upper > self.0 {
                res.render(StatusError::payload_too_large());
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        } else {
            res.render(StatusError::bad_request().brief("Request body size is unknown."));
            ctrl.skip_rest();
        }
    }
}
/// Create a new `MaxSize`.
#[inline]
pub fn max_size(size: u64) -> MaxSize {
    MaxSize(size)
}

#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[handler]
    async fn hello() -> &'static str {
        "hello"
    }

    #[tokio::test]
    async fn test_size_limiter() {
        let limit_handler = MaxSize(32);
        let router = Router::new()
            .hoop(limit_handler)
            .push(Router::with_path("hello").post(hello));
        let service = Service::new(router);

        let content = TestClient::post("http://127.0.0.1:5801/hello")
            .text("abc")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(content, "hello");

        let res = TestClient::post("http://127.0.0.1:5801/hello")
            .text("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
