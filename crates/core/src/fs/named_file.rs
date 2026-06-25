use std::borrow::Cow;
use std::cmp;
use std::fs::Metadata;
use std::io::{Read as StdRead, Seek as StdSeek, SeekFrom};
use std::ops::{Deref, DerefMut};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use enumflags2::{BitFlags, bitflags};
use headers::*;
use mime::Mime;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use tokio::fs::File;
#[allow(unused_imports)]
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::{ChunkedFile, ChunkedState};
use crate::http::body::ResBody;
use crate::http::header::{
    CONTENT_DISPOSITION, CONTENT_ENCODING, CONTENT_TYPE, IF_NONE_MATCH, RANGE,
};
use crate::http::mime::{detect_text_mime, fill_mime_charset_if_need, is_charset_required_mime};
use crate::http::{HttpRange, Request, Response, StatusCode, StatusError};
use crate::{Depot, Error, Result, Writer, async_trait};

const CHUNK_SIZE: u64 = 1024 * 1024;
const PRELOAD_THRESHOLD: u64 = 1024 * 1024;
const RFC5987_ATTR_CHAR_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'%')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'{')
    .add(b'}');

#[bitflags(default = Etag | LastModified | ContentDisposition)]
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum Flag {
    Etag = 0b0001,
    LastModified = 0b0010,
    ContentDisposition = 0b0100,
}

/// A file with an associated name and metadata for HTTP serving.
///
/// `NamedFile` wraps a file handle with HTTP-specific functionality including:
///
/// - Automatic MIME type detection based on file extension
/// - ETag generation for caching
/// - Last-Modified header support
/// - Content-Disposition header for downloads
/// - HTTP Range request support for partial content
/// - Chunked transfer for large files
///
/// # Opening Files
///
/// Files can be opened directly or through a builder:
///
/// ```
/// use salvo_core::fs::NamedFile;
///
/// async fn examples() {
///     // Simple open
///     let file = NamedFile::open("document.pdf").await;
///
///     // Builder pattern for more control
///     let file = NamedFile::builder("document.pdf")
///         .attached_name("report.pdf")
///         .buffer_size(65536)
///         .preload_threshold(262144)
///         .build()
///         .await;
/// }
/// ```
///
/// # Using as a Response
///
/// `NamedFile` implements [`Writer`], so it can be returned directly from handlers:
///
/// ```ignore
/// #[handler]
/// async fn download(res: &mut Response) -> Result<NamedFile> {
///     NamedFile::open("./files/document.pdf").await
/// }
/// ```
///
/// # Content-Disposition
///
/// By default, text, images, video, and audio files are served with
/// `Content-Disposition: inline`, while other files use `attachment`.
/// Use [`NamedFileBuilder::attached_name`] to force a download with a specific filename.
///
/// # Caching Headers
///
/// By default, `NamedFile` generates `ETag` and `Last-Modified` headers
/// and respects conditional request headers (`If-None-Match`, `If-Modified-Since`, etc.).
/// These can be disabled via [`use_etag()`](NamedFile::use_etag) and
/// [`use_last_modified()`](NamedFile::use_last_modified).
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
    /// Pre-read content for small files, avoiding ChunkedFile + spawn_blocking overhead.
    preread: Option<Bytes>,
}

/// Builder for constructing [`NamedFile`] instances with custom configuration.
///
/// The builder pattern allows customizing various aspects of file serving:
///
/// - MIME content type
/// - Content-Disposition (inline vs attachment)
/// - Download filename
/// - Buffer size for chunked reading
/// - Preload threshold for small-file responses
/// - ETag and Last-Modified header generation
///
/// # Example
///
/// ```ignore
/// use salvo_core::fs::NamedFile;
///
/// let file = NamedFile::builder("./data/export.csv")
///     .attached_name("data-export-2024.csv")  // Force download with this name
///     .content_type("text/csv".parse().unwrap())
///     .buffer_size(131072)  // 128KB chunks
///     .preload_threshold(262144)  // Preload files up to 256KB
///     .use_etag(true)
///     .build()
///     .await?;
/// ```
#[derive(Clone, Debug)]
pub struct NamedFileBuilder {
    path: PathBuf,
    attached_name: Option<String>,
    disposition_type: Option<String>,
    content_type: Option<mime::Mime>,
    content_encoding: Option<String>,
    buffer_size: Option<u64>,
    preload_threshold: Option<u64>,
    flags: BitFlags<Flag>,
}
impl NamedFileBuilder {
    /// Sets attached filename and returns `Self`.
    #[inline]
    #[must_use]
    pub fn attached_name<T: Into<String>>(mut self, attached_name: T) -> Self {
        self.attached_name = Some(attached_name.into());
        self.flags.insert(Flag::ContentDisposition);
        self
    }

    /// Sets disposition encoding and returns `Self`.
    #[inline]
    #[must_use]
    pub fn disposition_type<T: Into<String>>(mut self, disposition_type: T) -> Self {
        self.disposition_type = Some(disposition_type.into());
        self.flags.insert(Flag::ContentDisposition);
        self
    }

    /// Disable `Content-Disposition` header.
    ///
    /// By default, the `Content-Disposition` header is enabled.
    #[inline]
    pub fn disable_content_disposition(&mut self) {
        self.flags.remove(Flag::ContentDisposition);
    }

    /// Sets content type and returns `Self`.
    #[inline]
    #[must_use]
    pub fn content_type(mut self, content_type: mime::Mime) -> Self {
        self.content_type = Some(content_type);
        self
    }

    /// Sets content encoding and returns `Self`.
    #[inline]
    #[must_use]
    pub fn content_encoding<T: Into<String>>(mut self, content_encoding: T) -> Self {
        self.content_encoding = Some(content_encoding.into());
        self
    }

    /// Sets chunk buffer size and returns `Self`.
    ///
    /// This controls the maximum chunk size used when a file is streamed. It does not change the
    /// small-file preload threshold. Use [`Self::preload_threshold`] to configure that separately.
    #[inline]
    #[must_use]
    pub fn buffer_size(mut self, buffer_size: u64) -> Self {
        self.buffer_size = Some(buffer_size);
        self
    }

    /// Sets small-file preload threshold and returns `Self`.
    ///
    /// Files whose size is less than or equal to this threshold are read during build and sent from
    /// memory. Larger files are streamed in chunks using [`Self::buffer_size`]. Set this to `0` to
    /// disable preloading for non-empty files.
    #[inline]
    #[must_use]
    pub fn preload_threshold(mut self, threshold: u64) -> Self {
        self.preload_threshold = Some(threshold);
        self
    }

    /// Specifies whether to use ETag or not.
    ///
    /// Default is true.
    #[inline]
    #[must_use]
    pub fn use_etag(mut self, value: bool) -> Self {
        if value {
            self.flags.insert(Flag::Etag);
        } else {
            self.flags.remove(Flag::Etag);
        }
        self
    }

    /// Specifies whether to use Last-Modified or not.
    ///
    /// Default is true.
    #[inline]
    #[must_use]
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
        let Self {
            path,
            content_type,
            content_encoding,
            buffer_size,
            preload_threshold,
            disposition_type,
            attached_name,
            flags,
        } = self;

        let buf_size = buffer_size.unwrap_or(CHUNK_SIZE).max(1);
        let preload_threshold = preload_threshold.unwrap_or(PRELOAD_THRESHOLD);

        // Determine what charset detection is needed before the blocking call.
        let inferred_mime = content_type
            .clone()
            .or_else(|| mime_infer::from_path(&path).first());
        // When a content encoding is set, the on-disk bytes are the *encoded*
        // (e.g. gzip) payload of a precompressed sidecar file. Sniffing a charset
        // or text mime from those compressed bytes yields a bogus result (the
        // compressed blob is not valid UTF-8), so the wrong `charset=` would be
        // attached to the `Content-Type` and the client mojibakes the decoded
        // text. Skip content-based detection in that case.
        let is_encoded = content_encoding.is_some();
        let needs_charset = !is_encoded
            && inferred_mime
                .as_ref()
                .map(|m| is_charset_required_mime(m) && m.get_param("charset").is_none())
                .unwrap_or(false);
        let needs_detect = !is_encoded && content_type.is_none() && path.extension().is_none();

        let needs_detection_sample = needs_charset || needs_detect;

        // Single spawn_blocking: open + metadata + optional preread/detection sample.
        // This replaces 3-7 separate spawn_blocking calls with just 1.
        struct FileInfo {
            file: std::fs::File,
            metadata: Metadata,
            preread: Option<Vec<u8>>,
            detection_sample: Option<Vec<u8>>,
        }
        let blocking_path = path.clone();
        let info = tokio::task::spawn_blocking(move || -> std::io::Result<FileInfo> {
            let mut file = std::fs::File::open(&blocking_path)?;
            let metadata = file.metadata()?;
            let file_size = metadata.len();

            // For small files (size <= preload_threshold), read the entire content now.
            // This avoids ChunkedFile's spawn_blocking + into_std overhead later.
            let preread = if file_size <= preload_threshold {
                let mut buf = vec![0u8; file_size as usize];
                file.read_exact(&mut buf)?;
                Some(buf)
            } else {
                None
            };

            let detection_sample = if needs_detection_sample {
                if let Some(preread) = &preread {
                    Some(preread[..cmp::min(1024, preread.len())].to_vec())
                } else {
                    let mut sample = vec![0u8; cmp::min(1024, file_size) as usize];
                    file.read_exact(&mut sample)?;
                    file.seek(SeekFrom::Start(0))?;
                    Some(sample)
                }
            } else {
                None
            };

            Ok(FileInfo {
                file,
                metadata,
                preread,
                detection_sample,
            })
        })
        .await
        .map_err(|e| Error::other(format!("spawn_blocking: {e}")))?
        .map_err(Error::Io)?;

        let file = File::from_std(info.file);

        // Resolve content type, using preread bytes for charset detection if needed.
        let content_type = if let Some(mut mime) = inferred_mime {
            if needs_charset {
                let sample = info.detection_sample.as_deref().unwrap_or(&[]);
                fill_mime_charset_if_need(&mut mime, sample);
            }
            mime
        } else if needs_detect {
            let sample = info.detection_sample.as_deref().unwrap_or(&[]);
            detect_text_mime(sample).unwrap_or(mime::APPLICATION_OCTET_STREAM)
        } else {
            mime::APPLICATION_OCTET_STREAM
        };

        let preread = info.preread.map(Bytes::from);

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
            modified: info.metadata.modified().ok(),
            metadata: info.metadata,
            content_encoding,
            buffer_size: buf_size,
            flags,
            preread,
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
                .map(|file_name| file_name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into())
                .into(),
        };
        let quoted_filename = escape_quoted_filename(&attached_name);
        if quoted_filename == attached_name {
            format!(r#"attachment; filename="{quoted_filename}""#)
        } else {
            let encoded_filename =
                utf8_percent_encode(&attached_name, RFC5987_ATTR_CHAR_ENCODE_SET);
            format!(
                r#"attachment; filename="{quoted_filename}"; filename*=UTF-8''{encoded_filename}"#
            )
        }
        .parse::<HeaderValue>()
        .map_err(Error::other)?
    } else {
        disposition_type
            .parse::<HeaderValue>()
            .map_err(Error::other)?
    };
    Ok(content_disposition)
}

fn escape_quoted_filename(filename: &str) -> String {
    let mut escaped = String::with_capacity(filename.len());
    for ch in filename.chars() {
        match ch {
            '"' | '\\' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            '\t' => escaped.push(' '),
            ch if ch.is_ascii_control() || !ch.is_ascii() => escaped.push('_'),
            ch => escaped.push(ch),
        }
    }
    escaped
}
impl NamedFile {
    /// Creates a new [`NamedFileBuilder`].
    #[inline]
    pub fn builder(path: impl Into<PathBuf>) -> NamedFileBuilder {
        NamedFileBuilder {
            path: path.into(),
            attached_name: None,
            disposition_type: None,
            content_type: None,
            content_encoding: None,
            buffer_size: None,
            preload_threshold: None,
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
    pub async fn open<P>(path: P) -> Result<Self>
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
    /// By default, the `Content-Disposition` header is enabled.
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

            let dur = match mtime.duration_since(UNIX_EPOCH) {
                Ok(dur) => dur,
                Err(err) => {
                    tracing::warn!(
                        error = ?err,
                        path = %self.path.display(),
                        "skip file etag for modification time before unix epoch"
                    );
                    return None;
                }
            };
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
    /// Specifies whether to use ETag or not.
    ///
    /// Default is true.
    #[inline]
    pub fn use_etag(&mut self, value: bool) {
        if value {
            self.flags.insert(Flag::Etag);
        } else {
            self.flags.remove(Flag::Etag);
        }
    }

    /// Get last modified value.
    #[inline]
    pub fn last_modified(&self) -> Option<SystemTime> {
        self.modified
    }

    fn encodable_last_modified(&self, mtime: SystemTime) -> Option<SystemTime> {
        if let Err(err) = mtime.duration_since(UNIX_EPOCH) {
            tracing::warn!(
                error = ?err,
                path = %self.path.display(),
                "skip file last-modified header for modification time before unix epoch"
            );
            None
        } else {
            Some(mtime)
        }
    }
    /// Specifies whether to use Last-Modified or not.
    ///
    /// Default is true.
    #[inline]
    pub fn use_last_modified(&mut self, value: bool) {
        if value {
            self.flags.insert(Flag::LastModified);
        } else {
            self.flags.remove(Flag::LastModified);
        }
    }
    /// Consume self and send content to [`Response`].
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
            let since: SystemTime = since.into();
            since < http_date_precision(*last_modified)
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
            let since: SystemTime = since.into();
            since >= http_date_precision(*last_modified)
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
        if let Some(lm) = last_modified.and_then(|lm| self.encodable_last_modified(lm)) {
            res.headers_mut().typed_insert(LastModified::from(lm));
        }
        if let Some(etag) = etag {
            res.headers_mut().typed_insert(etag);
        }
        res.headers_mut().typed_insert(AcceptRanges::bytes());

        let mut length = self.metadata.len();
        if let Some(content_encoding) = &self.content_encoding {
            res.headers_mut()
                .insert(CONTENT_ENCODING, content_encoding.clone());
        }
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

        let file_size = self.metadata.len();
        let mut offset = 0;
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
            // Range request
            res.status_code(StatusCode::PARTIAL_CONTENT);
            // Single source of truth for the byte count, clamped to the file so the
            // `Content-Range`, `Content-Length` and the body always agree.
            let total_size = length.min(file_size.saturating_sub(offset));
            match ContentRange::bytes(offset..offset.saturating_add(total_size), file_size) {
                Ok(content_range) => {
                    res.headers_mut().typed_insert(content_range);
                }
                Err(e) => {
                    tracing::error!(error = ?e, "set file's content range failed");
                }
            }
            res.headers_mut().typed_insert(ContentLength(total_size));

            // Fast path: slice from preread bytes if available
            if let Some(preread) = self.preread {
                let end = cmp::min(offset.saturating_add(total_size) as usize, preread.len());
                let start = cmp::min(offset as usize, end);
                res.replace_body(ResBody::Once(preread.slice(start..end)));
            } else {
                let reader = ChunkedFile {
                    offset,
                    total_size,
                    read_size: 0,
                    state: ChunkedState::File(Some(self.file.into_std().await)),
                    buffer_size: self.buffer_size,
                };
                res.stream(reader);
            }
        } else {
            // Full file response
            res.status_code(StatusCode::OK);
            res.headers_mut().typed_insert(ContentLength(length));

            // Fast path: send preread bytes directly — zero spawn_blocking calls
            if let Some(preread) = self.preread {
                res.replace_body(ResBody::Once(preread));
            } else {
                let reader = ChunkedFile {
                    offset,
                    state: ChunkedState::File(Some(self.file.into_std().await)),
                    total_size: length,
                    read_size: 0,
                    buffer_size: self.buffer_size,
                };
                res.stream(reader);
            }
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

fn http_date_precision(time: SystemTime) -> SystemTime {
    match time.duration_since(UNIX_EPOCH) {
        Ok(dur) => UNIX_EPOCH + Duration::from_secs(dur.as_secs()),
        Err(err) => {
            let dur = err.duration();
            let secs = dur.as_secs() + u64::from(dur.subsec_nanos() > 0);
            UNIX_EPOCH
                .checked_sub(Duration::from_secs(secs))
                .unwrap_or(time)
        }
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
    use super::*;

    #[test]
    fn content_disposition_escapes_quoted_filename() {
        let value = build_content_disposition(
            "ignored.txt",
            &mime::APPLICATION_OCTET_STREAM,
            None,
            Some("report\"\\\r\n.txt"),
        )
        .unwrap();

        assert_eq!(
            value.to_str().unwrap(),
            r#"attachment; filename="report\"\\__.txt"; filename*=UTF-8''report%22%5C%0D%0A.txt"#
        );
    }

    #[tokio::test]
    async fn precompressed_file_does_not_sniff_charset_from_encoded_bytes() {
        use std::io::Write as _;

        // Simulate a `.js.gz` sidecar: the on-disk bytes are a gzip payload, which
        // is not valid UTF-8. Without the fix, charset detection runs on these
        // compressed bytes and attaches a bogus `charset=` to the text/javascript
        // content type, causing the client to mojibake the decoded source.
        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        file.write_all(&[
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xab, 0xe2, 0x80, 0x9c,
            0xc3, 0xa9, 0xb4, 0xd6, 0xfe, 0x00,
        ])
        .expect("write gzip-like bytes");
        file.flush().expect("flush");

        let named = NamedFile::builder(file.path())
            .content_type("text/javascript".parse().expect("parse mime"))
            .content_encoding("gzip")
            .build()
            .await
            .expect("build named file");

        // No charset must be sniffed from the encoded payload.
        assert_eq!(named.content_type().get_param("charset"), None);
        assert_eq!(
            named.content_encoding().map(|v| v.to_str().unwrap()),
            Some("gzip")
        );
    }

    #[tokio::test]
    async fn buffer_size_does_not_raise_preload_threshold() {
        use std::io::Write as _;

        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        let bytes = vec![b'a'; (PRELOAD_THRESHOLD + 1) as usize];
        file.write_all(&bytes).expect("write file");
        file.flush().expect("flush");

        let named = NamedFile::builder(file.path())
            .buffer_size(PRELOAD_THRESHOLD * 2)
            .build()
            .await
            .expect("build named file");

        assert_eq!(named.buffer_size, PRELOAD_THRESHOLD * 2);
        assert!(named.preread.is_none());
    }

    #[tokio::test]
    async fn preload_threshold_can_be_configured() {
        use std::io::Write as _;

        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        let bytes = vec![b'a'; (PRELOAD_THRESHOLD + 1) as usize];
        file.write_all(&bytes).expect("write file");
        file.flush().expect("flush");

        let named = NamedFile::builder(file.path())
            .preload_threshold(PRELOAD_THRESHOLD + 1)
            .build()
            .await
            .expect("build named file");

        assert_eq!(
            named.preread.as_ref().map(Bytes::len),
            Some((PRELOAD_THRESHOLD + 1) as usize)
        );
    }

    #[tokio::test]
    async fn preload_threshold_zero_still_detects_extensionless_text_mime() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let path = temp_dir.path().join("README");
        std::fs::write(&path, b"plain text content").expect("write extensionless file");

        let named = NamedFile::builder(&path)
            .preload_threshold(0)
            .build()
            .await
            .expect("build named file");

        assert_eq!(named.content_type().type_(), mime::TEXT);
        assert_eq!(named.content_type().subtype(), mime::PLAIN);
        assert!(named.preread.is_none());
    }

    #[tokio::test]
    async fn preload_threshold_zero_still_sniffs_charset() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let path = temp_dir.path().join("data.json");
        std::fs::write(&path, br#"{"message":"hello"}"#).expect("write json file");

        let named = NamedFile::builder(&path)
            .preload_threshold(0)
            .build()
            .await
            .expect("build named file");

        assert_eq!(named.content_type().type_(), mime::APPLICATION);
        assert_eq!(named.content_type().subtype(), mime::JSON);
        assert_eq!(
            named
                .content_type()
                .get_param("charset")
                .map(|v| v.as_str()),
            Some("utf-8")
        );
        assert!(named.preread.is_none());
    }

    #[tokio::test]
    async fn zero_buffer_size_is_clamped() {
        use std::io::Write as _;

        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        file.write_all(b"hello").expect("write file");
        file.flush().expect("flush");

        let named = NamedFile::builder(file.path())
            .buffer_size(0)
            .build()
            .await
            .expect("build named file");

        assert_eq!(named.buffer_size, 1);
    }

    #[tokio::test]
    async fn etag_returns_none_for_pre_epoch_modified_time() {
        use std::io::Write as _;
        use std::time::Duration;

        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        file.write_all(b"hello").expect("write file");
        file.flush().expect("flush");

        let mut named = NamedFile::builder(file.path())
            .build()
            .await
            .expect("build named file");
        named.modified = Some(UNIX_EPOCH - Duration::from_secs(1));

        assert_eq!(named.etag(), None);
    }

    #[tokio::test]
    async fn send_skips_last_modified_for_pre_epoch_modified_time() {
        use std::io::Write as _;
        use std::time::Duration;

        use crate::http::header::{IF_MODIFIED_SINCE, LAST_MODIFIED};
        use crate::http::{HeaderMap, Response};

        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        file.write_all(b"hello").expect("write file");
        file.flush().expect("flush");

        let mut named = NamedFile::builder(file.path())
            .build()
            .await
            .expect("build named file");
        let pre_epoch = UNIX_EPOCH - Duration::from_secs(1);
        named.modified = Some(pre_epoch);
        named.use_etag(false);
        assert_eq!(named.last_modified(), Some(pre_epoch));

        let mut headers = HeaderMap::new();
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:00 GMT"),
        );
        let mut res = Response::new();
        named.send(&headers, &mut res).await;

        assert_eq!(res.status_code, Some(StatusCode::NOT_MODIFIED));
        assert!(!res.headers().contains_key(LAST_MODIFIED));
    }

    #[tokio::test]
    async fn send_if_modified_since_uses_http_date_precision() {
        use std::io::Write as _;
        use std::time::Duration;

        use crate::http::header::IF_MODIFIED_SINCE;
        use crate::http::{HeaderMap, Response};

        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        file.write_all(b"hello").expect("write file");
        file.flush().expect("flush");

        let mut named = NamedFile::builder(file.path())
            .build()
            .await
            .expect("build named file");
        named.modified =
            Some(UNIX_EPOCH + Duration::from_secs(100) + Duration::from_nanos(500_000_000));
        named.use_etag(false);

        let mut headers = HeaderMap::new();
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:01:40 GMT"),
        );
        let mut res = Response::new();
        named.send(&headers, &mut res).await;

        assert_eq!(res.status_code, Some(StatusCode::NOT_MODIFIED));
    }

    #[tokio::test]
    async fn send_if_unmodified_since_uses_http_date_precision() {
        use std::io::Write as _;
        use std::time::Duration;

        use crate::http::header::IF_UNMODIFIED_SINCE;
        use crate::http::{HeaderMap, Response};

        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        file.write_all(b"hello").expect("write file");
        file.flush().expect("flush");

        let mut named = NamedFile::builder(file.path())
            .build()
            .await
            .expect("build named file");
        named.modified =
            Some(UNIX_EPOCH + Duration::from_secs(100) + Duration::from_nanos(500_000_000));
        named.use_etag(false);

        let mut headers = HeaderMap::new();
        headers.insert(
            IF_UNMODIFIED_SINCE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:01:40 GMT"),
        );
        let mut res = Response::new();
        named.send(&headers, &mut res).await;

        assert_eq!(res.status_code, Some(StatusCode::OK));
    }

    #[test]
    fn content_disposition_preserves_non_ascii_with_filename_star() {
        let value = build_content_disposition(
            "ignored.txt",
            &mime::APPLICATION_OCTET_STREAM,
            None,
            Some("报告.csv"),
        )
        .unwrap();

        assert_eq!(
            value.to_str().unwrap(),
            "attachment; filename=\"__.csv\"; filename*=UTF-8''%E6%8A%A5%E5%91%8A.csv"
        );
    }
}
