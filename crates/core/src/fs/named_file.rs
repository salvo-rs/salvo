use std::borrow::Cow;
use std::cmp;
use std::fs::Metadata;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use enumflags2::{BitFlags, bitflags};
use headers::*;
use tokio::fs::File;

use super::{ChunkedFile, ChunkedState};
use crate::http::header::{
    CONTENT_DISPOSITION, CONTENT_ENCODING, CONTENT_TYPE, IF_NONE_MATCH, RANGE,
};
use crate::http::{HttpRange, Mime, Request, Response, StatusCode, StatusError};
use crate::{Depot, Error, Result, Writer, async_trait};

const CHUNK_SIZE: u64 = 1024 * 1024;

#[bitflags(default = Etag | LastModified | ContentDisposition)]
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum Flag {
    Etag = 0b0001,
    LastModified = 0b0010,
    ContentDisposition = 0b0100,
}

/// A file with an associated name.
///
/// This struct represents a file with an associated name. It provides methods for opening and sending the file,
/// as well as setting various headers such as `Content-Type` and `Content-Disposition`.
///
/// # Examples
///
/// ```
/// use salvo_core::fs::NamedFile;
/// async fn open() {
///     let file = NamedFile::open("foo.txt").await;
/// }
///
#[derive(Debug)]
pub struct NamedFile {
    path: PathBuf,
    file: File,
    modified: Option<SystemTime>,
    buffer_size: u64,
    metadata: Metadata,
    flags: BitFlags<Flag>,
    content_type: mime::Mime,
    content_disposition: Option<HeaderValue>,
    content_encoding: Option<HeaderValue>,
}

/// Builder for build [`NamedFile`].
#[derive(Clone)]
pub struct NamedFileBuilder {
    path: PathBuf,
    attached_name: Option<String>,
    disposition_type: Option<String>,
    content_type: Option<mime::Mime>,
    content_encoding: Option<String>,
    buffer_size: Option<u64>,
    flags: BitFlags<Flag>,
}
impl NamedFileBuilder {
    /// Sets attached filename and returns `Self`.
    #[inline]
    pub fn attached_name<T: Into<String>>(mut self, attached_name: T) -> Self {
        self.attached_name = Some(attached_name.into());
        self.flags.insert(Flag::ContentDisposition);
        self
    }

    /// Sets disposition encoding and returns `Self`.
    #[inline]
    pub fn disposition_type<T: Into<String>>(mut self, disposition_type: T) -> Self {
        self.disposition_type = Some(disposition_type.into());
        self.flags.insert(Flag::ContentDisposition);
        self
    }

    /// Disable `Content-Disposition` header.
    ///
    /// By default Content-Disposition` header is enabled.
    #[inline]
    pub fn disable_content_disposition(&mut self) {
        self.flags.remove(Flag::ContentDisposition);
    }

    /// Sets content type and returns `Self`.
    #[inline]
    pub fn content_type(mut self, content_type: mime::Mime) -> Self {
        self.content_type = Some(content_type);
        self
    }

    /// Sets content encoding and returns `Self`.
    #[inline]
    pub fn content_encoding<T: Into<String>>(mut self, content_encoding: T) -> Self {
        self.content_encoding = Some(content_encoding.into());
        self
    }

    /// Sets buffer size and returns `Self`.
    #[inline]
    pub fn buffer_size(mut self, buffer_size: u64) -> Self {
        self.buffer_size = Some(buffer_size);
        self
    }

    ///Specifies whether to use ETag or not.
    ///
    /// Default is true.
    #[inline]
    pub fn use_etag(mut self, value: bool) -> Self {
        if value {
            self.flags.insert(Flag::Etag);
        } else {
            self.flags.remove(Flag::Etag);
        }
        self
    }

    ///Specifies whether to use Last-Modified or not.
    ///
    ///Default is true.
    #[inline]
    pub fn use_last_modified(mut self, value: bool) -> Self {
        if value {
            self.flags.insert(Flag::LastModified);
        } else {
            self.flags.remove(Flag::LastModified);
        }
        self
    }

    /// Build a new `NamedFile` and send it.
    pub async fn send(self, req_headers: &HeaderMap, res: &mut Response) {
        if !self.path.exists() {
            res.render(StatusError::not_found());
        } else {
            match self.build().await {
                Ok(file) => file.send(req_headers, res).await,
                Err(_) => res.render(StatusError::internal_server_error()),
            }
        }
    }

    /// Build a new [`NamedFile`].
    pub async fn build(self) -> Result<NamedFile> {
        let NamedFileBuilder {
            path,
            content_type,
            content_encoding,
            buffer_size,
            disposition_type,
            attached_name,
            flags,
        } = self;

        let file = File::open(&path).await?;
        let content_type = content_type.unwrap_or_else(|| {
            let ct = mime_infer::from_path(&path).first_or_octet_stream();
            let ftype = ct.type_();
            let stype = ct.subtype();
            if (ftype == mime::TEXT || stype == mime::JSON || stype == mime::JAVASCRIPT)
                && ct.get_param(mime::CHARSET).is_none()
            {
                //TODO: auto detect charset
                format!("{ct}; charset=utf-8")
                    .parse::<mime::Mime>()
                    .unwrap_or(ct)
            } else {
                ct
            }
        });
        let metadata = file.metadata().await?;
        let modified = metadata.modified().ok();
        let content_encoding = match content_encoding {
            Some(content_encoding) => Some(
                content_encoding
                    .parse::<HeaderValue>()
                    .map_err(Error::other)?,
            ),
            None => None,
        };

        let mut content_disposition = None;
        if attached_name.is_some() || disposition_type.is_some() {
            content_disposition = Some(build_content_disposition(
                &path,
                &content_type,
                disposition_type.as_deref(),
                attached_name.as_deref(),
            )?);
        }
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
fn build_content_disposition(
    file_path: impl AsRef<Path>,
    content_type: &Mime,
    disposition_type: Option<&str>,
    attached_name: Option<&str>,
) -> Result<HeaderValue> {
    let disposition_type = disposition_type.unwrap_or_else(|| {
        if attached_name.is_some() {
            "attachment"
        } else {
            match (content_type.type_(), content_type.subtype()) {
                (mime::IMAGE | mime::TEXT | mime::VIDEO | mime::AUDIO, _)
                | (_, mime::JAVASCRIPT | mime::JSON) => "inline",
                _ => "attachment",
            }
        }
    });
    let content_disposition = if disposition_type == "attachment" {
        let attached_name = match attached_name {
            Some(attached_name) => Cow::Borrowed(attached_name),
            None => file_path
                .as_ref()
                .file_name()
                .map(|file_name| file_name.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".into())
                .into(),
        };
        format!(r#"attachment; filename="{attached_name}""#)
            .parse::<HeaderValue>()
            .map_err(Error::other)?
    } else {
        disposition_type
            .parse::<HeaderValue>()
            .map_err(Error::other)?
    };
    Ok(content_disposition)
}
impl NamedFile {
    /// Create new [`NamedFileBuilder`].
    #[inline]
    pub fn builder(path: impl Into<PathBuf>) -> NamedFileBuilder {
        NamedFileBuilder {
            path: path.into(),
            attached_name: None,
            disposition_type: None,
            content_type: None,
            content_encoding: None,
            buffer_size: None,
            flags: BitFlags::default(),
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
    pub async fn open<P>(path: P) -> Result<NamedFile>
    where
        P: Into<PathBuf> + Send,
    {
        Self::builder(path).build().await
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
    /// Sets the MIME Content-Type for serving this file. By default
    /// the Content-Type is inferred from the filename extension.
    #[inline]
    pub fn set_content_type(&mut self, content_type: mime::Mime) {
        self.content_type = content_type;
    }

    /// Get Content-Disposition value.
    #[inline]
    pub fn content_disposition(&self) -> Option<&HeaderValue> {
        self.content_disposition.as_ref()
    }
    /// Sets the `Content-Disposition` for serving this file. This allows
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
        self.content_disposition = Some(content_disposition);
        self.flags.insert(Flag::ContentDisposition);
    }

    /// Disable `Content-Disposition` header.
    ///
    /// By default Content-Disposition` header is enabled.
    #[inline]
    pub fn disable_content_disposition(&mut self) {
        self.flags.remove(Flag::ContentDisposition);
    }

    /// Get content encoding value reference.
    #[inline]
    pub fn content_encoding(&self) -> Option<&HeaderValue> {
        self.content_encoding.as_ref()
    }
    /// Sets content encoding for serving this file
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
    pub fn use_etag(&mut self, value: bool) {
        if value {
            self.flags.insert(Flag::Etag);
        } else {
            self.flags.remove(Flag::Etag);
        }
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
    pub fn use_last_modified(&mut self, value: bool) {
        if value {
            self.flags.insert(Flag::LastModified);
        } else {
            self.flags.remove(Flag::LastModified);
        }
    }
    ///Consume self and send content to [`Response`].
    pub async fn send(mut self, req_headers: &HeaderMap, res: &mut Response) {
        let etag = if self.flags.contains(Flag::Etag) {
            self.etag()
        } else {
            None
        };
        let last_modified = if self.flags.contains(Flag::LastModified) {
            self.last_modified()
        } else {
            None
        };

        // check preconditions
        let precondition_failed = if !any_match(etag.as_ref(), req_headers) {
            true
        } else if let (Some(last_modified), Some(since)) =
            (&last_modified, req_headers.typed_get::<IfUnmodifiedSince>())
        {
            !since.precondition_passes(*last_modified)
        } else {
            false
        };

        // check last modified
        let not_modified = if !none_match(etag.as_ref(), req_headers) {
            true
        } else if req_headers.contains_key(IF_NONE_MATCH) {
            false
        } else if let (Some(last_modified), Some(since)) =
            (&last_modified, req_headers.typed_get::<IfModifiedSince>())
        {
            !since.is_modified(*last_modified)
        } else {
            false
        };

        if self.flags.contains(Flag::ContentDisposition) {
            if let Some(content_disposition) = self.content_disposition.take() {
                res.headers_mut()
                    .insert(CONTENT_DISPOSITION, content_disposition);
            } else if !res.headers().contains_key(CONTENT_DISPOSITION) {
                // skip to set CONTENT_DISPOSITION header if it is already set.
                match build_content_disposition(&self.path, &self.content_type, None, None) {
                    Ok(content_disposition) => {
                        res.headers_mut()
                            .insert(CONTENT_DISPOSITION, content_disposition);
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "build file's content disposition failed");
                    }
                }
            }
        }
        if !res.headers().contains_key(CONTENT_TYPE) {
            res.headers_mut()
                .typed_insert(ContentType::from(self.content_type.clone()));
        }
        if let Some(lm) = last_modified {
            res.headers_mut().typed_insert(LastModified::from(lm));
        }
        if let Some(etag) = self.etag() {
            res.headers_mut().typed_insert(etag);
        }
        res.headers_mut().typed_insert(AcceptRanges::bytes());

        let mut length = self.metadata.len();
        if let Some(content_encoding) = &self.content_encoding {
            res.headers_mut()
                .insert(CONTENT_ENCODING, content_encoding.clone());
        }
        let mut offset = 0;

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

        if offset != 0 || length != self.metadata.len() || range.is_some() {
            res.status_code(StatusCode::PARTIAL_CONTENT);
            match ContentRange::bytes(offset..offset + length, self.metadata.len()) {
                Ok(content_range) => {
                    res.headers_mut().typed_insert(content_range);
                }
                Err(e) => {
                    tracing::error!(error = ?e, "set file's content ranage failed");
                }
            }
            let reader = ChunkedFile {
                offset,
                total_size: cmp::min(length, self.metadata.len()),
                read_size: 0,
                state: ChunkedState::File(Some(self.file.into_std().await)),
                buffer_size: self.buffer_size,
            };
            res.headers_mut()
                .typed_insert(ContentLength(reader.total_size));
            res.stream(reader);
        } else {
            res.status_code(StatusCode::OK);
            let reader = ChunkedFile {
                offset,
                state: ChunkedState::File(Some(self.file.into_std().await)),
                total_size: length,
                read_size: 0,
                buffer_size: self.buffer_size,
            };
            res.headers_mut().typed_insert(ContentLength(length));
            res.stream(reader);
        }
    }
}

#[async_trait]
impl Writer for NamedFile {
    async fn write(self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        self.send(req.headers(), res).await;
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
