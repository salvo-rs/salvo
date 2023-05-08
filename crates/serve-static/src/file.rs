use std::path::PathBuf;

use salvo_core::fs::{NamedFile, NamedFileBuilder};
use salvo_core::http::{Request, Response, StatusError};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Writer};

/// StaticFile
#[derive(Clone)]
pub struct StaticFile(NamedFileBuilder);

impl StaticFile {
    /// Create a new `StaticFile`.
    #[inline]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        StaticFile(NamedFile::builder(path))
    }

    /// During the file chunk read, the maximum read size at one time will affect the
    /// access experience and the demand for server memory.
    ///
    /// Please set it according to your own situation.
    ///
    /// The default is 1M.
    #[inline]
    pub fn chunk_size(self, size: u64) -> Self {
        Self(self.0.buffer_size(size))
    }
}

#[async_trait]
impl Handler for StaticFile {
    #[inline]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        match self.0.clone().build().await {
            Ok(file) => file.write(req, depot, res).await,
            Err(_) => {
                res.render(StatusError::not_found());
            }
        }
        ctrl.skip_rest();
    }
}
