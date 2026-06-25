use std::io::SeekFrom;
use std::time::SystemTime;

use headers::*;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::http::header::{IF_NONE_MATCH, RANGE};
use crate::http::{HttpRange, Request, Response, StatusCode, StatusError};
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

    /// Consume self and send content to [`Response`].
    pub async fn send(mut self, req_headers: &HeaderMap, res: &mut Response) {
        // check preconditions
        let precondition_failed = if !any_match(self.etag.as_ref(), req_headers) {
            true
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
        } else if req_headers.contains_key(IF_NONE_MATCH) {
            false
        } else if let (Some(last_modified), Some(since)) = (
            &self.last_modified,
            req_headers.typed_get::<IfModifiedSince>(),
        ) {
            !since.is_modified(*last_modified)
        } else {
            false
        };

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
            res.status_code(StatusCode::NOT_MODIFIED);
            return;
        }

        let mut offset = 0;
        let mut length = self.length;
        let mut is_partial = false;
        // check for range header
        if let Some(range) = req_headers.get(RANGE) {
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
        self.send(req.headers(), res).await;
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
