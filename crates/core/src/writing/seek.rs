use std::io::SeekFrom;
use std::time::SystemTime;

use headers::*;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::http::header::{IF_NONE_MATCH, IF_RANGE, RANGE};
use crate::http::{HttpRange, Method, Request, Response, StatusCode, StatusError};
use crate::{Depot, Writer, async_trait};

/// `ReadSeeker` is used to write data to [`Response`] from a reader which implements [`AsyncRead`]
/// and [`AsyncSeek`].
///
/// # Example
/// ```
/// use salvo_core::prelude::*;
/// use salvo_core::writing::ReadSeeker;
///
/// #[handler]
/// async fn video_stream(req: &mut Request, res: &mut Response) {
///     let file = tokio::fs::File::open("video.mp4").await.unwrap();
///     res.add_header("Content-Type", "video/mp4", true).unwrap();
///     let length = file.metadata().await.unwrap().len();
///     ReadSeeker::new(file, length).send(req.headers(), res).await;
/// }
/// ```
#[derive(Debug)]
pub struct ReadSeeker<R> {
    reader: R,
    length: u64,
    last_modified: Option<SystemTime>,
    etag: Option<ETag>,
}

impl<R> ReadSeeker<R>
where
    R: AsyncSeek + AsyncRead + Unpin + Send + 'static,
{
    /// Create a new [`ReadSeeker`] from a reader which implements [`AsyncRead`] and [`AsyncSeek`].
    pub fn new(reader: R, length: u64) -> Self {
        Self {
            reader,
            length,
            last_modified: None,
            etag: None,
        }
    }

    /// Set the last modified time for the response.
    #[must_use]
    pub fn last_modified(mut self, time: SystemTime) -> Self {
        self.last_modified = Some(time);
        self
    }

    /// Set the ETag header for the response.
    #[must_use]
    pub fn etag(mut self, etag: ETag) -> Self {
        self.etag = Some(etag);
        self
    }

    /// Consume self and send content to [`Response`] with GET semantics.
    ///
    /// Use the [`Writer`] implementation to apply the actual request method.
    pub async fn send(self, req_headers: &HeaderMap, res: &mut Response) {
        self.send_inner(req_headers, res, &Method::GET).await;
    }

    /// Send only the headers for a HEAD request, preserving conditional request
    /// handling while ignoring `Range`. The reader is not read or sought.
    pub async fn send_head(self, req_headers: &HeaderMap, res: &mut Response) {
        self.send_inner(req_headers, res, &Method::HEAD).await;
    }

    async fn send_inner(mut self, req_headers: &HeaderMap, res: &mut Response, method: &Method) {
        let send_body = method != Method::HEAD;
        let retrieval = method == Method::GET || method == Method::HEAD;
        if !send_body {
            res.take_body();
        }
        // check preconditions
        let precondition_failed = if !any_match(self.etag.as_ref(), req_headers) {
            true
        } else if req_headers.contains_key(crate::http::header::IF_MATCH) {
            // If-Match takes precedence over If-Unmodified-Since, including
            // the wildcard existence precondition (RFC 9110 section 13.1.4).
            false
        } else if let (Some(last_modified), Some(since)) = (
            &self.last_modified,
            req_headers.typed_get::<IfUnmodifiedSince>(),
        ) {
            !since.precondition_passes(*last_modified)
        } else {
            false
        };

        // check last modified
        let not_modified = if !none_match(self.etag.as_ref(), req_headers) {
            true
        } else if req_headers.contains_key(IF_NONE_MATCH) || !retrieval {
            false
        } else if let (Some(last_modified), Some(since)) = (
            &self.last_modified,
            req_headers.typed_get::<IfModifiedSince>(),
        ) {
            !since.is_modified(*last_modified)
        } else {
            false
        };

        // If-Range requires a strong tag or an exact date match. A failed or
        // invalid validator ignores Range, including an unsatisfiable range.
        let range_allowed = !req_headers.contains_key(IF_RANGE)
            || req_headers.typed_get::<IfRange>().is_some_and(|if_range| {
                !if_range.is_modified(self.etag.as_ref(), None)
                    || self
                        .last_modified
                        .is_some_and(|modified| if_range == IfRange::date(modified))
            });

        if let Some(lm) = self.last_modified {
            res.headers_mut().typed_insert(LastModified::from(lm));
        }
        if let Some(etag) = self.etag {
            res.headers_mut().typed_insert(etag);
        }
        res.headers_mut().typed_insert(AcceptRanges::bytes());

        // Conditional request handling must precede Range processing: per RFC 7232
        // a `304 Not Modified` / `412 Precondition Failed` takes priority over the
        // `206`/`416` produced by a Range request.
        if precondition_failed {
            res.status_code(StatusCode::PRECONDITION_FAILED);
            return;
        } else if not_modified {
            res.status_code(if retrieval {
                StatusCode::NOT_MODIFIED
            } else {
                StatusCode::PRECONDITION_FAILED
            });
            return;
        }

        if !send_body {
            res.status_code(StatusCode::OK);
            res.headers_mut().typed_insert(ContentLength(self.length));
            return;
        }

        let mut offset = 0;
        let mut length = self.length;
        let mut is_partial = false;
        // check for range header
        if let Some(range) = req_headers
            .get(RANGE)
            .filter(|_| method == Method::GET && range_allowed)
        {
            let Ok(range) = range.to_str() else {
                res.status_code(StatusCode::BAD_REQUEST);
                return;
            };
            match HttpRange::parse(range, length) {
                // A single range is served as `206 Partial Content`.
                Ok(ranges) if ranges.len() == 1 => {
                    offset = ranges[0].start;
                    length = ranges[0].length;
                    is_partial = true;
                }
                // Multiple ranges would require a `multipart/byteranges` body, which
                // is not supported here. Per RFC 7233 the server may ignore the Range
                // header and return the full `200 OK` representation instead.
                Ok(ranges) if ranges.len() > 1 => {}
                // Empty / unsatisfiable range.
                _ => {
                    res.headers_mut()
                        .typed_insert(ContentRange::unsatisfied_bytes(length));
                    res.status_code(StatusCode::RANGE_NOT_SATISFIABLE);
                    return;
                }
            }
        }

        if is_partial {
            res.status_code(StatusCode::PARTIAL_CONTENT);
            // Derive the byte count from a single source and clamp defensively so the
            // declared `Content-Range`, `Content-Length` and the streamed body always
            // agree on the number of bytes.
            let content_length = length.min(self.length.saturating_sub(offset));
            match ContentRange::bytes(offset..offset.saturating_add(content_length), self.length) {
                Ok(content_range) => {
                    res.headers_mut().typed_insert(content_range);
                }
                Err(e) => {
                    tracing::error!(error = ?e, "set file's content range failed");
                }
            }
            if let Err(e) = self.reader.seek(SeekFrom::Start(offset)).await {
                tracing::error!(error = ?e, "seek file failed");
                res.render(StatusError::bad_request().brief("seek file failed"));
                return;
            }
            res.headers_mut()
                .typed_insert(ContentLength(content_length));
            res.stream(ReaderStream::new(self.reader.take(content_length)));
        } else {
            res.status_code(StatusCode::OK);
            res.headers_mut().typed_insert(ContentLength(self.length));
            res.stream(ReaderStream::new(self.reader.take(self.length)));
        }
    }
}

#[async_trait]
impl<R> Writer for ReadSeeker<R>
where
    R: AsyncSeek + AsyncRead + Unpin + Send + 'static,
{
    #[inline]
    async fn write(self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        self.send_inner(req.headers(), res, req.method()).await;
    }
}

/// Returns true if `req_headers` has no `If-Match` header or one which matches `etag`.
fn any_match(etag: Option<&ETag>, req_headers: &HeaderMap) -> bool {
    match req_headers.typed_get::<IfMatch>() {
        None => true,
        Some(if_match) => {
            if if_match == IfMatch::any() {
                true
            } else if let Some(etag) = etag {
                if_match.precondition_passes(etag)
            } else {
                false
            }
        }
    }
}

/// Returns true if `req_headers` doesn't have an `If-None-Match` header matching `req`.
fn none_match(etag: Option<&ETag>, req_headers: &HeaderMap) -> bool {
    match req_headers.typed_get::<IfNoneMatch>() {
        None => true,
        Some(if_none_match) => {
            if if_none_match == IfNoneMatch::any() {
                false
            } else if let Some(etag) = etag {
                if_none_match.precondition_passes(etag)
            } else {
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::time::Duration;

    use super::*;
    use crate::test::ResponseExt;

    #[tokio::test]
    async fn if_range_requires_a_matching_strong_validator() {
        let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        for (validator, tag, date, range, expected) in [
            (
                None,
                Some("\"new\""),
                true,
                "bytes=4-",
                StatusCode::PARTIAL_CONTENT,
            ),
            (
                Some("\"new\""),
                Some("\"new\""),
                true,
                "bytes=4-",
                StatusCode::PARTIAL_CONTENT,
            ),
            (
                Some("\"old\""),
                Some("\"new\""),
                true,
                "bytes=4-",
                StatusCode::OK,
            ),
            (
                Some("W/\"new\""),
                Some("\"new\""),
                true,
                "bytes=4-",
                StatusCode::OK,
            ),
            (
                Some("W/\"new\""),
                Some("W/\"new\""),
                true,
                "bytes=4-",
                StatusCode::OK,
            ),
            (Some("\"new\""), None, true, "bytes=4-", StatusCode::OK),
            (
                Some("Tue, 14 Nov 2023 22:13:20 GMT"),
                None,
                true,
                "bytes=4-",
                StatusCode::PARTIAL_CONTENT,
            ),
            (
                Some("Tue, 14 Nov 2023 22:13:19 GMT"),
                None,
                true,
                "bytes=4-",
                StatusCode::OK,
            ),
            (
                Some("Tue, 14 Nov 2023 22:13:21 GMT"),
                None,
                true,
                "bytes=4-",
                StatusCode::OK,
            ),
            (
                Some("Tue, 14 Nov 2023 22:13:20 GMT"),
                Some("\"new\""),
                false,
                "bytes=4-",
                StatusCode::OK,
            ),
            (
                Some("invalid"),
                Some("\"new\""),
                true,
                "bytes=4-",
                StatusCode::OK,
            ),
            (
                Some("\"old\""),
                Some("\"new\""),
                true,
                "bytes=999-",
                StatusCode::OK,
            ),
            (
                Some("\"old\""),
                Some("\"new\""),
                true,
                "invalid",
                StatusCode::OK,
            ),
            (
                Some("\"new\""),
                Some("\"new\""),
                true,
                "bytes=999-",
                StatusCode::RANGE_NOT_SATISFIABLE,
            ),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(RANGE, range.parse().unwrap());
            if let Some(validator) = validator {
                headers.insert("if-range", validator.parse().unwrap());
            }
            let mut reader = ReadSeeker::new(Cursor::new(b"new-content"), 11);
            if let Some(tag) = tag {
                reader = reader.etag(tag.parse().unwrap());
            }
            if date {
                reader = reader.last_modified(modified);
            }
            let mut response = Response::new();
            reader.send(&headers, &mut response).await;
            assert_eq!(
                response.status_code,
                Some(expected),
                "{validator:?}, {tag:?}, {date}, {range}"
            );
            if expected == StatusCode::OK {
                assert!(!response.headers().contains_key("content-range"));
                assert_eq!(response.headers()["content-length"], "11");
                assert_eq!(response.take_string().await.unwrap(), "new-content");
            } else if expected == StatusCode::PARTIAL_CONTENT {
                assert_eq!(response.headers()["content-range"], "bytes 4-10/11");
                assert_eq!(response.headers()["content-length"], "7");
                assert_eq!(response.take_string().await.unwrap(), "content");
            } else {
                assert_eq!(response.headers()["content-range"], "bytes */11");
                assert!(response.body.is_none());
            }
        }
    }

    #[tokio::test]
    async fn if_range_does_not_override_other_preconditions() {
        for (condition, tag, expected) in [
            ("if-none-match", "\"new\"", StatusCode::NOT_MODIFIED),
            ("if-match", "\"old\"", StatusCode::PRECONDITION_FAILED),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(condition, tag.parse().unwrap());
            headers.insert("if-range", "\"old\"".parse().unwrap());
            headers.insert(RANGE, "bytes=999-".parse().unwrap());
            let mut response = Response::new();
            ReadSeeker::new(Cursor::new(b"new-content"), 11)
                .etag("\"new\"".parse().unwrap())
                .send(&headers, &mut response)
                .await;
            assert_eq!(response.status_code, Some(expected));
            assert!(response.body.is_none());
            assert!(!response.headers().contains_key("content-range"));
        }
    }
}
