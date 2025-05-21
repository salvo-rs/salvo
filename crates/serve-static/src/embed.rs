use std::borrow::Cow;
use std::marker::PhantomData;

use rust_embed::{EmbeddedFile, Metadata, RustEmbed};
use salvo_core::handler::Handler;
use salvo_core::http::header::{
    ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_NONE_MATCH, RANGE,
};
use salvo_core::http::headers::{ContentLength, ContentRange, HeaderMapExt};
use salvo_core::http::{HeaderValue, HttpRange, Mime, Request, Response, StatusCode};
use salvo_core::{Depot, FlowCtrl, IntoVecString, async_trait};

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
pub fn render_embedded_file(
    file: EmbeddedFile,
    req: &Request,
    res: &mut Response,
    mime: Option<Mime>,
) {
    let EmbeddedFile { data, metadata, .. } = file;
    render_embedded_data(data, &metadata, req, res, mime);
}

fn render_embedded_data(
    data: Cow<'static, [u8]>,
    metadata: &Metadata,
    req: &Request,
    res: &mut Response,
    mime_override: Option<Mime>,
) {
    // Determine Content-Type once
    let effective_mime = mime_override
        .unwrap_or_else(|| mime_infer::from_path(req.uri().path()).first_or_octet_stream());
    res.headers_mut().insert(
        CONTENT_TYPE,
        effective_mime
            .as_ref()
            .parse()
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );

    // ETag generation and If-None-Match check
    let hash = hex::encode(metadata.sha256_hash());
    if req
        .headers()
        .get(IF_NONE_MATCH)
        .map(|etag| etag.to_str().unwrap_or("000000").eq(&hash))
        .unwrap_or(false)
    {
        res.status_code(StatusCode::NOT_MODIFIED);
        return;
    }

    // Set ETag for all successful responses (200 or 206)
    if let Ok(etag_val) = hash.parse() {
        res.headers_mut().insert(ETAG, etag_val);
    } else {
        tracing::error!("Failed to parse etag hash: {}", hash);
    }

    // Indicate that byte ranges are accepted
    res.headers_mut()
        .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));

    let total_data_len = data.len() as u64;
    let mut is_partial_content = false;
    let mut range_to_send: Option<(u64, u64)> = None; // (start_offset, length_of_part)

    let req_headers = req.headers();
    if let Some(range_header_val) = req_headers.get(RANGE) {
        if let Ok(range_str) = range_header_val.to_str() {
            match HttpRange::parse(range_str, total_data_len) {
                Ok(ranges) if !ranges.is_empty() => {
                    // Successfully parsed and satisfiable range(s). We only handle the first one.
                    let first_range = &ranges[0]; // HttpRange ensures start + length <= total_data_len
                    is_partial_content = true;
                    range_to_send = Some((first_range.start, first_range.length));

                    res.status_code(StatusCode::PARTIAL_CONTENT);
                    match ContentRange::bytes(
                        first_range.start..(first_range.start + first_range.length),
                        total_data_len,
                    ) {
                        Ok(content_range_header) => {
                            res.headers_mut().typed_insert(content_range_header);
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "Failed to create Content-Range header");
                            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                            return;
                        }
                    }
                }
                Err(_) => {
                    // HttpRange::parse returns Err if the range is unsatisfiable or malformed.
                    res.headers_mut()
                        .typed_insert(ContentRange::unsatisfied_bytes(total_data_len));
                    res.status_code(StatusCode::RANGE_NOT_SATISFIABLE);
                    return;
                }
                Ok(_) => {
                    // Parsed, but no valid ranges. Treat as full content.
                    // is_partial_content remains false.
                }
            }
        } else {
            // Failed to convert Range header to string (e.g., invalid UTF-8)
            res.status_code(StatusCode::BAD_REQUEST);
            return;
        }
    }

    if is_partial_content {
        if let Some((offset, length)) = range_to_send {
            // Ensure the range is valid before slicing. HttpRange::parse should guarantee this.
            let end_offset = offset
                .checked_add(length)
                .expect("Range calculation overflowed");
            if end_offset <= total_data_len {
                // Check to prevent panic on slice
                let partial_data_vec = data[offset as usize..end_offset as usize].to_vec();
                res.headers_mut().typed_insert(ContentLength(length));
                let _ = res.write_body(partial_data_vec); // write_body can take Vec<u8>
            } else {
                // This should ideally be caught by HttpRange::parse or ContentRange::bytes
                tracing::error!("Calculated range exceeds data bounds after HttpRange::parse");
                res.headers_mut()
                    .typed_insert(ContentRange::unsatisfied_bytes(total_data_len));
                res.status_code(StatusCode::RANGE_NOT_SATISFIABLE);
                // Clear content length if we are not sending a body for this error
                res.headers_mut().remove(CONTENT_LENGTH);
            }
        } else {
            // Should not happen if is_partial_content is true.
            tracing::error!("is_partial_content is true but range_to_send is None");
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        }
    } else {
        // Serve full content
        res.status_code(StatusCode::OK); // Ensure OK status
        res.headers_mut()
            .typed_insert(ContentLength(total_data_len));
        match data {
            Cow::Borrowed(d) => {
                let _ = res.write_body(d);
            }
            Cow::Owned(o) => {
                let _ = res.write_body(o);
            }
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
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
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
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
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
