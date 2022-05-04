use std::cmp;
use std::fs::Metadata;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs::File;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use async_trait::async_trait;
use bitflags::bitflags;
use headers::*;
use mime_guess::from_path;

use super::{ChunkedState, FileChunk};
use crate::http::header::{self, CONTENT_DISPOSITION, CONTENT_ENCODING};
use crate::http::{HttpRange, Request, Response, StatusCode, StatusError};
use crate::{Depot, Error, Result, Writer};

const CHUNK_SIZE: u64 = 1024 * 1024;

bitflags! {
    pub(crate) struct Flags: u8 {
        const ETAG = 0b0000_0001;
        const LAST_MODIFIED = 0b0000_0010;
        const CONTENT_DISPOSITION = 0b0000_0100;
    }
}

impl Default for Flags {
    fn default() -> Self {
        Flags::all()
    }
}

/// A file with an associated name.
#[derive(Debug)]
pub struct NamedFile {
    path: PathBuf,
    file: File,
    modified: Option<SystemTime>,
    buffer_size: u64,
    metadata: Metadata,
    flags: Flags,
    content_type: mime::Mime,
    content_disposition: HeaderValue,
    content_encoding: Option<HeaderValue>,
}

/// Builder for build [`NamedFile`].
#[derive(Clone)]
pub struct NamedFileBuilder {
    path: PathBuf,
    attached_filename: Option<String>,
    disposition_type: Option<String>,
    content_type: Option<mime::Mime>,
    content_encoding: Option<String>,
    content_disposition: Option<String>,
    buffer_size: Option<u64>,
    flags: Flags,
}
impl NamedFileBuilder {
    /// Set attached filename and returns `Self`.
    #[inline]
    pub fn with_attached_filename<T: Into<String>>(mut self, attached_filename: T) -> Self {
        self.attached_filename = Some(attached_filename.into());
        self
    }
    /// Set disposition encoding and returns `Self`.
    #[inline]
    pub fn with_disposition_type<T: Into<String>>(mut self, disposition_type: T) -> Self {
        self.disposition_type = Some(disposition_type.into());
        self
    }
    /// Set content type and returns `Self`.
    #[inline]
    pub fn with_content_type<T: Into<mime::Mime>>(mut self, content_type: T) -> Self {
        self.content_type = Some(content_type.into());
        self
    }
    /// Set content encoding and returns `Self`.
    #[inline]
    pub fn with_content_encoding<T: Into<String>>(mut self, content_encoding: T) -> Self {
        self.content_encoding = Some(content_encoding.into());
        self
    }
    /// Set buffer size and returns `Self`.
    #[inline]
    pub fn with_buffer_size(mut self, buffer_size: u64) -> Self {
        self.buffer_size = Some(buffer_size);
        self
    }
    /// Specifies whether to use ETag or not.
    ///
    /// Default is true.
    #[inline]
    pub fn use_etag(mut self, value: bool) -> Self {
        self.flags.set(Flags::ETAG, value);
        self
    }
    /// Build a new `NamedFile` and send it.
    pub async fn send(self, req: &mut Request, res: &mut Response) {
        if !self.path.exists() {
            res.set_status_error(StatusError::not_found());
        } else {
            match self.build().await {
                Ok(file) => file.send(req, res).await,
                Err(_) => res.set_status_error(StatusError::internal_server_error()),
            }
        }
    }
    /// Build a new [`NamedFile`].
    pub async fn build(self) -> Result<NamedFile> {
        let NamedFileBuilder {
            path,
            content_type,
            content_encoding,
            content_disposition,
            buffer_size,
            disposition_type,
            attached_filename,
            flags,
        } = self;

        let file = File::open(&path).await?;
        let content_type = content_type.unwrap_or_else(|| {
            let ct = from_path(&path).first_or_octet_stream();
            let ftype = ct.type_();
            let stype = ct.subtype();
            if (ftype == mime::TEXT || stype == mime::JSON || stype == mime::JAVASCRIPT)
                && ct.get_param(mime::CHARSET).is_none()
            {
                //TODO: auto detect charset
                format!("{}; charset=utf-8", ct).parse::<mime::Mime>().unwrap_or(ct)
            } else {
                ct
            }
        });
        let content_disposition = content_disposition.unwrap_or_else(|| {
            disposition_type.unwrap_or_else(|| {
                let disposition_type = if attached_filename.is_some() {
                    "attachment"
                } else {
                    match (content_type.type_(), content_type.subtype()) {
                        (mime::IMAGE | mime::TEXT | mime::VIDEO | mime::AUDIO, _)
                        | (_, mime::JAVASCRIPT | mime::JSON) => "inline",
                        _ => "attachment",
                    }
                };
                if disposition_type == "attachment" {
                    let filename = match attached_filename {
                        Some(filename) => filename,
                        None => path
                            .file_name()
                            .map(|filename| filename.to_string_lossy().to_string())
                            .unwrap_or_else(|| "file".into()),
                    };
                    format!("attachment; filename={}", filename)
                } else {
                    disposition_type.into()
                }
            })
        });
        let content_disposition = content_disposition.parse::<HeaderValue>().map_err(Error::other)?;
        let metadata = file.metadata().await?;
        let modified = metadata.modified().ok();
        let content_encoding = match content_encoding {
            Some(content_encoding) => Some(content_encoding.parse::<HeaderValue>().map_err(Error::other)?),
            None => None,
        };

        Ok(NamedFile {
            path,
            file,
            content_type,
            content_disposition,
            metadata,
            modified,
            content_encoding,
            buffer_size: buffer_size.unwrap_or(CHUNK_SIZE),
            flags,
        })
    }
}

impl NamedFile {
    /// Create new [`NamedFileBuilder`].
    #[inline]
    pub fn builder(path: impl Into<PathBuf>) -> NamedFileBuilder {
        NamedFileBuilder {
            path: path.into(),
            attached_filename: None,
            disposition_type: None,
            content_type: None,
            content_encoding: None,
            content_disposition: None,
            buffer_size: None,
            flags: Flags::default(),
        }
    }

    /// Attempts to open a file in read-only mode.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::fs::NamedFile;
    /// # async fn open() {
    /// let file = NamedFile::open("foo.txt").await;
    /// # }
    /// ```
    #[inline]
    pub async fn open(path: impl Into<PathBuf>) -> Result<NamedFile> {
        Self::builder(path).build().await
    }

    /// Attempts to send a file. If file not exists, not found error will occur.
    pub async fn send_file(path: impl Into<PathBuf>, req: &mut Request, res: &mut Response) {
        let path = path.into();
        if !path.exists() {
            res.set_status_error(StatusError::not_found());
        } else {
            match Self::builder(path).build().await {
                Ok(file) => file.send(req, res).await,
                Err(_) => res.set_status_error(StatusError::internal_server_error()),
            }
        }
    }

    /// Returns reference to the underlying `File` object.
    #[inline]
    pub fn file(&self) -> &File {
        &self.file
    }

    /// Retrieve the path of this file.
    #[inline]
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Get content type value.
    #[inline]
    pub fn content_type(&self) -> &mime::Mime {
        &self.content_type
    }
    /// Set the MIME Content-Type for serving this file. By default
    /// the Content-Type is inferred from the filename extension.
    #[inline]
    pub fn set_content_type(&mut self, content_type: mime::Mime) {
        self.content_type = content_type;
    }

    /// Get Content-Disposition value.
    #[inline]
    pub fn content_disposition(&self) -> &HeaderValue {
        &self.content_disposition
    }
    /// Set the `Content-Disposition` for serving this file. This allows
    /// changing the inline/attachment disposition as well as the filename
    /// sent to the peer.
    ///
    /// By default the disposition is `inline` for text,
    /// image, and video content types, and `attachment` otherwise, and
    /// the filename is taken from the path provided in the `open` method
    /// after converting it to UTF-8 using
    /// [to_string_lossy](https://doc.rust-lang.org/std/ffi/struct.OsStr.html#method.to_string_lossy).
    #[inline]
    pub fn set_content_disposition(&mut self, content_disposition: HeaderValue) {
        self.content_disposition = content_disposition;
        self.flags.insert(Flags::CONTENT_DISPOSITION);
    }

    /// Disable `Content-Disposition` header.
    ///
    /// By default Content-Disposition` header is enabled.
    #[inline]
    pub fn disable_content_disposition(&mut self) {
        self.flags.remove(Flags::CONTENT_DISPOSITION);
    }

    /// Get content encoding value reference.
    #[inline]
    pub fn content_encoding(&self) -> Option<&HeaderValue> {
        self.content_encoding.as_ref()
    }
    /// Set content encoding for serving this file
    #[inline]
    pub fn set_content_encoding(&mut self, content_encoding: HeaderValue) {
        self.content_encoding = Some(content_encoding);
    }

    /// Get ETag value.
    pub fn etag(&self) -> Option<ETag> {
        // This etag format is similar to Apache's.
        self.modified.as_ref().and_then(|mtime| {
            let ino = {
                #[cfg(unix)]
                {
                    self.metadata.ino()
                }
                #[cfg(not(unix))]
                {
                    0
                }
            };

            let dur = mtime
                .duration_since(UNIX_EPOCH)
                .expect("modification time must be after epoch");
            let etag_str = format!(
                "\"{:x}-{:x}-{:x}-{:x}\"",
                ino,
                self.metadata.len(),
                dur.as_secs(),
                dur.subsec_nanos()
            );
            match etag_str.parse::<ETag>() {
                Ok(etag) => Some(etag),
                Err(e) => {
                    tracing::error!(error = ?e, etag = %etag_str, "set file's etag failed");
                    None
                }
            }
        })
    }
    ///Specifies whether to use ETag or not.
    ///
    ///Default is true.
    #[inline]
    pub fn use_etag(mut self, value: bool) {
        self.flags.set(Flags::ETAG, value);
    }

    /// GEt last_modified value.
    #[inline]
    pub fn last_modified(&self) -> Option<SystemTime> {
        self.modified
    }
    ///Specifies whether to use Last-Modified or not.
    ///
    ///Default is true.
    #[inline]
    pub fn use_last_modified(mut self, value: bool) -> Self {
        self.flags.set(Flags::LAST_MODIFIED, value);
        self
    }
    ///Consume self and send content to [`Response`].
    pub async fn send(self, req: &mut Request, res: &mut Response) {
        let etag = if self.flags.contains(Flags::ETAG) {
            self.etag()
        } else {
            None
        };
        let last_modified = if self.flags.contains(Flags::LAST_MODIFIED) {
            self.last_modified()
        } else {
            None
        };

        // check preconditions
        let precondition_failed = if !any_match(etag.as_ref(), req) {
            true
        } else if let (Some(ref last_modified), Some(since)) =
            (last_modified, req.headers().typed_get::<IfUnmodifiedSince>())
        {
            !since.precondition_passes(*last_modified)
        } else {
            false
        };

        // check last modified
        let not_modified = if !none_match(etag.as_ref(), req) {
            true
        } else if req.headers().contains_key(header::IF_NONE_MATCH) {
            false
        } else if let (Some(ref last_modified), Some(since)) =
            (last_modified, req.headers().typed_get::<IfModifiedSince>())
        {
            !since.is_modified(*last_modified)
        } else {
            false
        };

        res.headers_mut()
            .insert(CONTENT_DISPOSITION, self.content_disposition.clone());
        res.headers_mut()
            .typed_insert(ContentType::from(self.content_type.clone()));
        if let Some(lm) = last_modified {
            res.headers_mut().typed_insert(LastModified::from(lm));
        }
        if let Some(etag) = self.etag() {
            res.headers_mut().typed_insert(etag);
        }
        res.headers_mut().typed_insert(AcceptRanges::bytes());

        let mut length = self.metadata.len();
        if let Some(content_encoding) = &self.content_encoding {
            res.headers_mut().insert(CONTENT_ENCODING, content_encoding.clone());
        }
        let mut offset = 0;

        // check for range header
        // let mut range = None;
        if let Some(ranges) = req.headers().get(header::RANGE) {
            if let Ok(rangesheader) = ranges.to_str() {
                if let Ok(rangesvec) = HttpRange::parse(rangesheader, length) {
                    length = rangesvec[0].length;
                    offset = rangesvec[0].start;
                } else {
                    res.headers_mut().typed_insert(ContentRange::unsatisfied_bytes(length));
                    res.set_status_code(StatusCode::RANGE_NOT_SATISFIABLE);
                    return;
                };
            } else {
                res.set_status_code(StatusCode::BAD_REQUEST);
                return;
            };
        }

        if precondition_failed {
            res.set_status_code(StatusCode::PRECONDITION_FAILED);
            return;
        } else if not_modified {
            res.set_status_code(StatusCode::NOT_MODIFIED);
            return;
        }

        if offset != 0 || length != self.metadata.len() {
            res.set_status_code(StatusCode::PARTIAL_CONTENT);
            match ContentRange::bytes(offset..offset + length - 1, self.metadata.len()) {
                Ok(content_range) => {
                    res.headers_mut().typed_insert(content_range);
                }
                Err(e) => {
                    tracing::error!(error = ?e, "set file's content ranage failed");
                }
            }
            let reader = FileChunk {
                offset,
                chunk_size: cmp::min(length, self.metadata.len()),
                read_size: 0,
                state: ChunkedState::File(Some(self.file.into_std().await)),
                buffer_size: self.buffer_size,
            };
            res.headers_mut().typed_insert(ContentLength(reader.chunk_size));
            res.streaming(reader).ok();
        } else {
            res.set_status_code(StatusCode::OK);
            let reader = FileChunk {
                offset,
                state: ChunkedState::File(Some(self.file.into_std().await)),
                chunk_size: length,
                read_size: 0,
                buffer_size: self.buffer_size,
            };
            res.headers_mut().typed_insert(ContentLength(length - offset));
            res.streaming(reader).ok();
        }
    }
}

#[async_trait]
impl Writer for NamedFile {
    async fn write(mut self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        self.send(req, res).await;
    }
}

impl Deref for NamedFile {
    type Target = File;

    fn deref(&self) -> &File {
        &self.file
    }
}

impl DerefMut for NamedFile {
    fn deref_mut(&mut self) -> &mut File {
        &mut self.file
    }
}

/// Returns true if `req` has no `If-Match` header or one which matches `etag`.
fn any_match(etag: Option<&ETag>, req: &Request) -> bool {
    match req.headers().typed_get::<IfMatch>() {
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

/// Returns true if `req` doesn't have an `If-None-Match` header matching `req`.
fn none_match(etag: Option<&ETag>, req: &Request) -> bool {
    match req.headers().typed_get::<IfMatch>() {
        None => true,
        Some(if_match) => {
            if if_match == IfMatch::any() {
                false
            } else if let Some(etag) = etag {
                !if_match.precondition_passes(etag)
            } else {
                true
            }
        }
    }
}
