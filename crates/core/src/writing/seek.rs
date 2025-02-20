use std::cmp;
use std::io::SeekFrom;
use std::time::SystemTime;

use headers::*;
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::http::header::{IF_NONE_MATCH, RANGE};
use crate::http::{HttpRange, Request, Response, StatusCode, StatusError};
use crate::{Depot, Writer, async_trait};

/// `ReadSeeker` is used to write data to [`Response`] from a reader which implements [`AsyncRead`] and [`AsyncSeek`].
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
        ReadSeeker {
            reader,
            length,
            last_modified: None,
            etag: None,
        }
    }

    /// Set the last modified time for the response.
    pub fn last_modified(mut self, time: SystemTime) -> Self {
        self.last_modified = Some(time);
        self
    }

    /// Set the ETag header for the response.
    pub fn etag(mut self, etag: ETag) -> Self {
        self.etag = Some(etag);
        self
    }

    ///Consume self and send content to [`Response`].
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

        let mut offset = 0;
        let mut length = self.length;
        // check for range header
        let range = req_headers.get(RANGE);
        if let Some(range) = range {
            if let Ok(range) = range.to_str() {
                if let Ok(range) = HttpRange::parse(range, length) {
                    length = range[0].length;
                    offset = range[0].start;
                } else {
                    res.headers_mut()
                        .typed_insert(ContentRange::unsatisfied_bytes(length));
                    res.status_code(StatusCode::RANGE_NOT_SATISFIABLE);
                    return;
                };
            } else {
                res.status_code(StatusCode::BAD_REQUEST);
                return;
            };
        }

        if precondition_failed {
            res.status_code(StatusCode::PRECONDITION_FAILED);
            return;
        } else if not_modified {
            res.status_code(StatusCode::NOT_MODIFIED);
            return;
        }

        if offset != 0 || length != self.length || range.is_some() {
            res.status_code(StatusCode::PARTIAL_CONTENT);
            match ContentRange::bytes(offset..offset + length, self.length) {
                Ok(content_range) => {
                    res.headers_mut().typed_insert(content_range);
                }
                Err(e) => {
                    tracing::error!(error = ?e, "set file's content ranage failed");
                }
            }
            if let Err(e) = self.reader.seek(SeekFrom::Start(offset)).await {
                tracing::error!(error = ?e, "seek file failed");
                res.render(StatusError::bad_request().brief("seek file failed"));
                return;
            }
            res.headers_mut()
                .typed_insert(ContentLength(cmp::min(length, self.length)));
            res.stream(ReaderStream::new(self.reader));
        } else {
            res.status_code(StatusCode::OK);
            res.headers_mut().typed_insert(ContentLength(self.length));
            res.stream(ReaderStream::new(self.reader));
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
