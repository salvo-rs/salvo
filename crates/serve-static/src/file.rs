use std::path::PathBuf;

use salvo_core::fs::{NamedFile, NamedFileBuilder};
use salvo_core::http::{Method, Request, Response, StatusError};
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};

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
///     "Hello World"
/// }
///
/// let router = Router::new()
///     .get(hello)
///     .push(Router::with_path("favicon.ico").get(StaticFile::new("assets/favicon.ico")));
/// ```
#[derive(Clone, Debug)]
pub struct StaticFile(NamedFileBuilder);

impl StaticFile {
    /// Create a new `StaticFile` handler.
    #[inline]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(NamedFile::builder(path))
    }

    /// Set the chunk size for file reading.
    ///
    /// During file reading, the maximum read size at one time will affect the
    /// access experience and memory usage of the server.
    ///
    /// This controls streaming chunks and does not change `NamedFile`'s small-file preload
    /// threshold.
    ///
    /// Please set it according to your specific requirements.
    ///
    /// The default is 1MB.
    #[inline]
    #[must_use]
    pub fn chunk_size(self, size: u64) -> Self {
        Self(self.0.buffer_size(size))
    }

    /// Set the small-file preload threshold.
    ///
    /// Files whose size is less than or equal to this threshold are read during `NamedFile`
    /// construction and sent from memory. Larger files are streamed in chunks. Set this to `0` to
    /// disable preloading for non-empty files.
    #[inline]
    #[must_use]
    pub fn preload_threshold(self, threshold: u64) -> Self {
        Self(self.0.preload_threshold(threshold))
    }

    /// Set the `Content-Disposition` type, e.g. `inline` or `attachment`.
    ///
    /// By default the type is derived from the file's content type: `inline` for
    /// text, image, video and audio, `attachment` for everything else including
    /// XML-based documents such as `image/svg+xml`. Set this to `inline` only for
    /// files you trust — an SVG rendered inline runs its own script in the
    /// serving origin.
    #[inline]
    #[must_use]
    pub fn disposition_type(self, disposition_type: impl Into<String>) -> Self {
        Self(self.0.disposition_type(disposition_type))
    }

    /// Serve the file as a download under `name`, regardless of its content type.
    #[inline]
    #[must_use]
    pub fn attached_name(self, name: impl Into<String>) -> Self {
        Self(self.0.attached_name(name))
    }

    /// Specifies whether to send `X-Content-Type-Options: nosniff` or not.
    ///
    /// Default is true.
    #[inline]
    #[must_use]
    pub fn use_content_type_options(self, value: bool) -> Self {
        Self(self.0.use_content_type_options(value))
    }
}

#[async_trait]
impl Handler for StaticFile {
    #[inline]
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let mut builder = self.0.clone();
        if req.method() == Method::HEAD {
            builder = builder.preload_threshold(0);
        }
        match builder.build().await {
            Ok(file) if req.method() == Method::HEAD => file.send_head(req.headers(), res).await,
            Ok(file) => file.send(req.headers(), res).await,
            Err(_) => {
                res.render(StatusError::not_found());
            }
        }
        ctrl.skip_rest();
    }
}
