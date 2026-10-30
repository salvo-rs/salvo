use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;
use std::path::Path;

use rust_embed::{EmbeddedFile, Metadata, RustEmbed};
use salvo_core::fs::extension_content_encoding;
use salvo_core::handler::Handler;
use salvo_core::http::header::{
    ACCEPT_RANGES, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_NONE_MATCH, IF_RANGE, RANGE,
    X_CONTENT_TYPE_OPTIONS,
};
use salvo_core::http::headers::{
    ContentLength, ContentRange, ETag, HeaderMapExt, IfMatch, IfRange,
};
use salvo_core::http::mime::fill_mime_charset_if_need;
use salvo_core::http::{HeaderValue, HttpRange, Method, Mime, Request, Response, StatusCode};
use salvo_core::routing::{decode_url_path, normalize_url_path, redirect_to_dir_url};
use salvo_core::{Depot, FlowCtrl, IntoVecString, async_trait};

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
impl<T: Debug> Debug for StaticEmbed<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticEmbed")
            .field("defaults", &self.defaults)
            .field("fallback", &self.fallback)
            .finish()
    }
}

/// Create a new `StaticEmbed` handler for the given embedded asset type.
#[inline]
#[must_use]
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
    let EmbeddedFile { data, metadata } = file;
    render_embedded_data(data, &metadata, req, res, mime, req.uri().path());
}

/// Returns whether an `If-None-Match` header value matches the given hash.
///
/// Per RFC 7232 the header is `*` or a list of (possibly weak, `W/`-prefixed)
/// entity-tags. The opaque value of a quoted tag may itself contain commas, so
/// the list is parsed quote-aware rather than split on `,`. A bare (unquoted)
/// tag is also accepted for backward compatibility with the previously emitted
/// ETag.
fn if_none_match_matches(if_none_match: &str, hash: &str) -> bool {
    let mut rest = if_none_match.trim();
    if rest == "*" {
        return true;
    }
    while !rest.is_empty() {
        rest = rest.trim_start_matches([' ', '\t', ',']);
        // An entity-tag may carry a weak indicator; weak/strong both match here.
        let tag = rest.strip_prefix("W/").unwrap_or(rest);
        if let Some(after_open) = tag.strip_prefix('"') {
            // Quoted tag: the opaque value runs up to the next `"` and may
            // contain commas.
            match after_open.find('"') {
                Some(end) => {
                    if &after_open[..end] == hash {
                        return true;
                    }
                    rest = &after_open[end + 1..];
                }
                None => break, // malformed (unterminated quote)
            }
        } else {
            // Bare (legacy) token: runs up to the next comma.
            let end = tag.find(',').unwrap_or(tag.len());
            if tag[..end].trim() == hash {
                return true;
            }
            rest = &tag[end..];
        }
    }
    false
}

fn render_embedded_data(
    data: Cow<'static, [u8]>,
    metadata: &Metadata,
    req: &Request,
    res: &mut Response,
    mime: Option<Mime>,
    source_path: &str,
) {
    let retrieval = req.method() == Method::GET || req.method() == Method::HEAD;
    if req.method() == Method::HEAD {
        res.take_body();
    }

    // Some extensions describe an encoded representation, e.g. .svgz is SVG
    // inside gzip. Use the resolved asset name for defaults and fallbacks, and
    // preserve an explicit coding already configured by the caller. Do not
    // install an inferred coding until this response actually describes the
    // asset: a failed precondition or range can be rendered as plain HTML by
    // the service's catcher.
    let inferred_encoding = Path::new(source_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(extension_content_encoding)
        .filter(|_| !res.headers().contains_key(CONTENT_ENCODING));
    let set_inferred_encoding = |res: &mut Response| {
        if let Some(encoding) = inferred_encoding {
            res.headers_mut()
                .insert(CONTENT_ENCODING, HeaderValue::from_static(encoding));
        }
    };

    // Determine Content-Type once
    let content_type =
        if let Some(mut mime) = mime.or_else(|| mime_infer::from_path(req.uri().path()).first()) {
            // Encoded bytes cannot tell us the charset of the decoded text.
            if inferred_encoding.is_none() && !res.headers().contains_key(CONTENT_ENCODING) {
                fill_mime_charset_if_need(&mut mime, &data);
            }
            mime
        } else {
            mime::APPLICATION_OCTET_STREAM
        };

    res.headers_mut().insert(
        CONTENT_TYPE,
        content_type
            .as_ref()
            .parse()
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    res.headers_mut()
        .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));

    // ETag generation and If-None-Match check.
    let hash = hex::encode(metadata.sha256_hash());
    // RFC 7232 entity-tags are quoted; emit a proper `"<hash>"` so conforming
    // clients echo it verbatim.
    let etag = format!("\"{hash}\"");

    // Set the ETag before any early return so a `304 Not Modified` carries the
    // same validator a `200`/`206` would (RFC 7232 §4.1), matching the
    // file-serving path.
    if let Ok(etag_val) = etag.parse() {
        res.headers_mut().insert(ETAG, etag_val);
    } else {
        tracing::error!("Failed to parse etag: {etag}");
    }

    // Evaluate If-Match first: its strong comparison takes precedence over
    // cache revalidation and range selection for this existing representation.
    if let Some(if_match) = req.headers().typed_get::<IfMatch>()
        && res
            .headers()
            .typed_get::<ETag>()
            .is_none_or(|etag| !if_match.precondition_passes(&etag))
    {
        res.status_code(StatusCode::PRECONDITION_FAILED);
        return;
    }

    let not_modified = req
        .headers()
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| if_none_match_matches(value, &hash));
    if not_modified {
        if retrieval {
            set_inferred_encoding(res);
        }
        res.status_code(if retrieval {
            StatusCode::NOT_MODIFIED
        } else {
            StatusCode::PRECONDITION_FAILED
        });
        return;
    }

    // Indicate that byte ranges are accepted
    res.headers_mut()
        .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));

    let total_data_len = data.len() as u64;
    if req.method() == Method::HEAD {
        set_inferred_encoding(res);
        res.status_code(StatusCode::OK);
        res.headers_mut()
            .typed_insert(ContentLength(total_data_len));
        return;
    }
    let mut is_partial_content = false;
    let mut range_to_send: Option<(u64, u64)> = None; // (start_offset, length_of_part)

    let req_headers = req.headers();
    // Embedded assets expose only an ETag validator. Dates, weak tags and
    // invalid If-Range values must therefore fall back to the full response.
    let range_allowed = !req_headers.contains_key(IF_RANGE)
        || req_headers.typed_get::<IfRange>().is_some_and(|if_range| {
            !if_range.is_modified(res.headers().typed_get::<ETag>().as_ref(), None)
        });
    if let Some(range_header_val) = req_headers
        .get(RANGE)
        .filter(|_| req.method() == Method::GET && range_allowed)
    {
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
                // RFC 9110 section 14.2 requires ignoring unknown range units.
                Err(_)
                    if range_str
                        .split_once('=')
                        .is_some_and(|(unit, _)| !unit.eq_ignore_ascii_case("bytes")) => {}
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
                set_inferred_encoding(res);
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
        set_inferred_encoding(res);
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            _assets: PhantomData,
            defaults: vec![],
            fallback: None,
        }
    }

    /// Create a new `StaticEmbed` with defaults.
    #[inline]
    #[must_use]
    pub fn defaults(mut self, defaults: impl IntoVecString) -> Self {
        self.defaults = defaults.into_vec_string();
        self
    }

    /// Create a new `StaticEmbed` with fallback.
    #[inline]
    #[must_use]
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
        let decoded_path;
        let req_path = if let Some(rest) = req.params().tail() {
            rest
        } else {
            decoded_path = decode_url_path(req.uri().path());
            decoded_path.as_ref()
        };
        let req_path = normalize_url_path(req_path);
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
            if !fallback.is_empty()
                && let Some(file) = T::get(fallback)
            {
                embedded_file = Some(file);
                key_path = Cow::from(fallback);
            }
        }

        match embedded_file {
            Some(file) => {
                let mime = mime_infer::from_path(&*key_path).first();
                render_embedded_data(file.data, &file.metadata, req, res, mime, &key_path);
            }
            None => {
                res.status_code(StatusCode::NOT_FOUND);
            }
        }
    }
}

/// Handler for [`EmbeddedFile`].
pub struct EmbeddedFileHandler(pub EmbeddedFile);
impl Debug for EmbeddedFileHandler {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddedFileHandler").finish()
    }
}

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
        render_embedded_data(
            self.0.data.clone(),
            &self.0.metadata,
            req,
            res,
            None,
            req.uri().path(),
        );
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

#[cfg(test)]
mod tests {
    use super::if_none_match_matches;

    #[tokio::test]
    async fn embedded_if_match_precedes_cache_and_range_conditions() {
        use rust_embed::RustEmbed;
        use salvo_core::prelude::*;
        use salvo_core::test::TestClient;

        #[derive(RustEmbed)]
        #[folder = "test/static/"]
        struct Assets;

        let service =
            Service::new(Router::with_path("{*path}").goal(super::StaticEmbed::<Assets>::new()));
        let original = TestClient::get("http://localhost/test1.txt")
            .send(&service)
            .await;
        let etag = original.headers()["etag"].to_str().unwrap().to_owned();
        for (if_match, if_none_match, range, expected) in [
            (
                "\"old\"".to_owned(),
                None,
                None,
                StatusCode::PRECONDITION_FAILED,
            ),
            (
                format!("W/{etag}"),
                None,
                None,
                StatusCode::PRECONDITION_FAILED,
            ),
            (etag.clone(), None, None, StatusCode::OK),
            (format!("\"other\", {etag}"), None, None, StatusCode::OK),
            ("*".to_owned(), None, None, StatusCode::OK),
            (
                "\"old\"".to_owned(),
                Some(etag.clone()),
                Some("bytes=999999-"),
                StatusCode::PRECONDITION_FAILED,
            ),
            (
                etag.clone(),
                Some(etag.clone()),
                Some("bytes=999999-"),
                StatusCode::NOT_MODIFIED,
            ),
            (
                etag.clone(),
                None,
                Some("bytes=1-"),
                StatusCode::PARTIAL_CONTENT,
            ),
        ] {
            let mut request = TestClient::get("http://localhost/test1.txt").add_header(
                "if-match",
                if_match.clone(),
                true,
            );
            if let Some(tag) = if_none_match {
                request = request.add_header("if-none-match", tag, true);
            }
            if let Some(range) = range {
                request = request.add_header("range", range, true);
            }
            let response = request.send(&service).await;
            assert_eq!(response.status_code, Some(expected), "{if_match}");
            assert_eq!(response.headers()["etag"], etag);
            if expected == StatusCode::PRECONDITION_FAILED || expected == StatusCode::NOT_MODIFIED {
                assert!(!response.headers().contains_key("content-range"));
            }
        }
        for (if_match, expected) in [
            ("\"old\"".to_owned(), StatusCode::PRECONDITION_FAILED),
            (etag.clone(), StatusCode::OK),
        ] {
            let response = TestClient::head("http://localhost/test1.txt")
                .add_header("if-match", if_match, true)
                .add_header("range", "bytes=999999-", true)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(expected));
            assert!(response.body.is_none());
            assert!(!response.headers().contains_key("content-range"));
        }
    }

    #[tokio::test]
    async fn if_range_requires_a_matching_strong_etag() {
        use rust_embed::RustEmbed;
        use salvo_core::prelude::*;
        use salvo_core::test::{ResponseExt, TestClient};

        #[derive(RustEmbed)]
        #[folder = "test/static/"]
        struct Assets;

        let service =
            Service::new(Router::with_path("{*path}").get(super::StaticEmbed::<Assets>::new()));
        let mut original = TestClient::get("http://localhost/test1.txt")
            .send(&service)
            .await;
        let etag = original.headers()["etag"].to_str().unwrap().to_owned();
        let body = original.take_bytes(None).await.unwrap();
        for (validator, range, expected) in [
            (None, "bytes=1-", StatusCode::PARTIAL_CONTENT),
            (Some(etag.clone()), "bytes=1-", StatusCode::PARTIAL_CONTENT),
            (Some("\"old\"".into()), "bytes=1-", StatusCode::OK),
            (Some(format!("W/{etag}")), "bytes=1-", StatusCode::OK),
            (
                Some("Tue, 14 Nov 2023 22:13:20 GMT".into()),
                "bytes=1-",
                StatusCode::OK,
            ),
            (Some("invalid".into()), "bytes=1-", StatusCode::OK),
            (Some("\"old\"".into()), "bytes=999999-", StatusCode::OK),
            (Some("\"old\"".into()), "invalid", StatusCode::OK),
            (
                Some(etag.clone()),
                "bytes=999999-",
                StatusCode::RANGE_NOT_SATISFIABLE,
            ),
        ] {
            let mut request =
                TestClient::get("http://localhost/test1.txt").add_header("range", range, true);
            if let Some(validator) = &validator {
                request = request.add_header("if-range", validator, true);
            }
            let mut response = request.send(&service).await;
            assert_eq!(
                response.status_code,
                Some(expected),
                "{validator:?}, {range}"
            );
            assert_eq!(response.headers()["etag"], etag);
            if expected == StatusCode::OK {
                assert!(!response.headers().contains_key("content-range"));
                assert_eq!(response.headers()["content-length"], body.len().to_string());
                assert_eq!(response.take_bytes(None).await.unwrap(), body);
            } else if expected == StatusCode::PARTIAL_CONTENT {
                assert_eq!(
                    response.headers()["content-range"],
                    format!("bytes 1-{}/{}", body.len() - 1, body.len())
                );
                assert_eq!(
                    response.headers()["content-length"],
                    (body.len() - 1).to_string()
                );
                assert_eq!(response.take_bytes(None).await.unwrap(), body.slice(1..));
            } else {
                assert_eq!(
                    response.headers()["content-range"],
                    format!("bytes */{}", body.len())
                );
            }
        }
        let response = TestClient::get("http://localhost/test1.txt")
            .add_header("if-none-match", etag, true)
            .add_header("if-range", "\"old\"", true)
            .add_header("range", "bytes=999999-", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
        assert!(response.body.is_none());
    }

    #[test]
    fn if_none_match_handles_quoted_weak_and_lists() {
        let hash = "0a1b2c3d";
        // Quoted form (what the server now emits) and bare legacy form.
        assert!(if_none_match_matches("\"0a1b2c3d\"", hash));
        assert!(if_none_match_matches("0a1b2c3d", hash));
        // Weak validator and wildcard.
        assert!(if_none_match_matches("W/\"0a1b2c3d\"", hash));
        assert!(if_none_match_matches("*", hash));
        // Comma-separated list with surrounding whitespace.
        assert!(if_none_match_matches("\"other\", \"0a1b2c3d\"", hash));
        // Non-matches.
        assert!(!if_none_match_matches("\"deadbeef\"", hash));
        assert!(!if_none_match_matches("", hash));
        // A single quoted tag whose opaque value merely contains the hash after a
        // comma must NOT match (the comma is inside the quotes, not a separator).
        assert!(!if_none_match_matches("\"other,0a1b2c3d\"", hash));
        // ...but a genuine two-element list where the second tag's opaque value
        // contains a comma still matches the first.
        assert!(if_none_match_matches("\"0a1b2c3d\", \"x,y\"", hash));
    }
}
