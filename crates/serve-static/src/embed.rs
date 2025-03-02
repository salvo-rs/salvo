use std::borrow::Cow;
use std::marker::PhantomData;

use rust_embed::{EmbeddedFile, Metadata, RustEmbed};
use salvo_core::http::header::{CONTENT_TYPE, ETAG, IF_NONE_MATCH};
use salvo_core::http::{HeaderValue, Mime, Request, Response, StatusCode};
use salvo_core::{async_trait, Depot, FlowCtrl, IntoVecString};
use salvo_core::handler::{ Handler};

use super::{decode_url_path_safely, format_url_path_safely, join_path, redirect_to_dir_url};

/// Handler that serves embedded files using `rust-embed`.
///
/// This handler allows serving files embedded in the application binary,
/// which is useful for distributing a self-contained executable.
#[non_exhaustive]
#[derive(Default)]
pub struct StaticEmbed<T> {
    _assets: PhantomData<T>,
    /// Default file names list (e.g., "index.html")
    pub defaults: Vec<String>,
    /// Fallback file name used when the requested file isn't found
    pub fallback: Option<String>,
}

/// Create a new `StaticEmbed` handler for the given embedded asset type.
#[inline]
pub fn static_embed<T: RustEmbed>() -> StaticEmbed<T> {
    StaticEmbed {
        _assets: PhantomData,
        defaults: vec![],
        fallback: None,
    }
}

/// Render an [`EmbeddedFile`] to the [`Response`].
#[inline]
pub fn render_embedded_file(file: EmbeddedFile, req: &Request, res: &mut Response, mime: Option<Mime>) {
    let EmbeddedFile { data, metadata, .. } = file;
    render_embedded_data(data, &metadata, req, res, mime);
}

fn render_embedded_data(
    data: Cow<'static, [u8]>,
    metadata: &Metadata,
    req: &Request,
    res: &mut Response,
    mime: Option<Mime>,
) {
    let mime = mime.unwrap_or_else(|| mime_infer::from_path(req.uri().path()).first_or_octet_stream());
    res.headers_mut().insert(
        CONTENT_TYPE,
        mime.as_ref()
            .parse()
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );

    let hash = hex::encode(metadata.sha256_hash());
    // if etag is matched, return 304
    if req
        .headers()
        .get(IF_NONE_MATCH)
        .map(|etag| etag.to_str().unwrap_or("000000").eq(&hash))
        .unwrap_or(false)
    {
        res.status_code(StatusCode::NOT_MODIFIED);
        return;
    }

    // otherwise, return 200 with etag hash
    if let Ok(hash) = hash.parse() {
        res.headers_mut().insert(ETAG, hash);
    } else {
        tracing::error!("Failed to parse etag hash: {}", hash);
    }

    match data {
        Cow::Borrowed(data) => {
            let _ = res.write_body(data);
        }
        Cow::Owned(data) => {
            let _ = res.write_body(data);
        }
    }
}

impl<T> StaticEmbed<T>
where
    T: RustEmbed + Send + Sync + 'static,
{
    /// Create a new `StaticEmbed`.
    #[inline]
    pub fn new() -> Self {
        Self {
            _assets: PhantomData,
            defaults: vec![],
            fallback: None,
        }
    }

    /// Create a new `StaticEmbed` with defaults.
    #[inline]
    pub fn defaults(mut self, defaults: impl IntoVecString) -> Self {
        self.defaults = defaults.into_vec_string();
        self
    }

    /// Create a new `StaticEmbed` with fallback.
    #[inline]
    pub fn fallback(mut self, fallback: impl Into<String>) -> Self {
        self.fallback = Some(fallback.into());
        self
    }
}
#[async_trait]
impl<T> Handler for StaticEmbed<T>
where
    T: RustEmbed + Send + Sync + 'static,
{
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        let req_path = if let Some(rest) = req.params().tail() {
            rest
        } else {
            &*decode_url_path_safely(req.uri().path())
        };
        let req_path = format_url_path_safely(req_path);
        let mut key_path = Cow::Borrowed(&*req_path);
        let mut embedded_file = T::get(req_path.as_str());
        if embedded_file.is_none() {
            for ifile in &self.defaults {
                let ipath = join_path!(&req_path, ifile);
                if let Some(file) = T::get(&ipath) {
                    embedded_file = Some(file);
                    key_path = Cow::from(ipath);
                    break;
                }
            }
            if embedded_file.is_some() && !req_path.ends_with('/') && !req_path.is_empty() {
                redirect_to_dir_url(req.uri(), res);
                return;
            }
        }
        if embedded_file.is_none() {
            let fallback = self.fallback.as_deref().unwrap_or_default();
            if !fallback.is_empty() {
                if let Some(file) = T::get(fallback) {
                    embedded_file = Some(file);
                    key_path = Cow::from(fallback);
                }
            }
        }

        match embedded_file {
            Some(file) => {
                let mime = mime_infer::from_path(&*key_path).first_or_octet_stream();
                render_embedded_file(file, req, res, Some(mime));
            }
            None => {
                res.status_code(StatusCode::NOT_FOUND);
            }
        }
    }
}

/// Handler for [`EmbeddedFile`].
pub struct EmbeddedFileHandler(pub EmbeddedFile);

#[async_trait]
impl Handler for EmbeddedFileHandler {
    #[inline]
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        render_embedded_data(self.0.data.clone(), &self.0.metadata, req, res, None);
    }
}

/// Extension trait for [`EmbeddedFile`].
pub trait EmbeddedFileExt {
    /// Render the embedded file.
    fn render(self, req: &Request, res: &mut Response);
    /// Create a handler for the embedded file.
    fn into_handler(self) -> EmbeddedFileHandler;
}

impl EmbeddedFileExt for EmbeddedFile {
    #[inline]
    fn render(self, req: &Request, res: &mut Response) {
        render_embedded_file(self, req, res, None);
    }
    #[inline]
    fn into_handler(self) -> EmbeddedFileHandler {
        EmbeddedFileHandler(self)
    }
}
