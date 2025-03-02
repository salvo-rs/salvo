use std::path::PathBuf;

use salvo_core::fs::{NamedFile, NamedFileBuilder};
use salvo_core::http::{Request, Response, StatusError};
use salvo_core::{Depot, FlowCtrl, Handler, Writer, async_trait};

/// `StaticFile` is a handler that serves a single static file.
///
/// # Examples
///
/// ```
/// use salvo_core::prelude::*;
/// use salvo_serve_static::StaticFile;
///
/// #[handler]
/// async fn hello() -> &'static str {
///    "Hello World"
/// }
///
/// let router = Router::new()
///    .get(hello)
///    .push(Router::with_path("favicon.ico").get(StaticFile::new("assets/favicon.ico")));
/// ```
#[derive(Clone)]
pub struct StaticFile(NamedFileBuilder);

impl StaticFile {
    /// Create a new `StaticFile` handler.
    #[inline]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        StaticFile(NamedFile::builder(path))
    }

    /// Set the chunk size for file reading.
    ///
    /// During file reading, the maximum read size at one time will affect the
    /// access experience and memory usage of the server.
    ///
    /// Please set it according to your specific requirements.
    ///
    /// The default is 1MB.
    #[inline]
    pub fn chunk_size(self, size: u64) -> Self {
        Self(self.0.buffer_size(size))
    }
}

#[async_trait]
impl Handler for StaticFile {
    #[inline]
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        match self.0.clone().build().await {
            Ok(file) => file.write(req, depot, res).await,
            Err(_) => {
                res.render(StatusError::not_found());
            }
        }
        ctrl.skip_rest();
    }
}
