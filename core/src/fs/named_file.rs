use std::fs::{File, Metadata};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{cmp, io};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use async_trait::async_trait;
use bitflags::bitflags;
use headers::*;
use mime_guess::from_path;

use super::FileChunk;
use crate::http::header;
use crate::http::header::{CONTENT_DISPOSITION, CONTENT_ENCODING};
use crate::http::range::HttpRange;
use crate::http::{Request, Response, StatusCode};
use crate::Depot;
use crate::Writer;

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
    pub buffer_size: u64,
    pub(crate) metadata: Metadata,
    pub(crate) flags: Flags,
    pub(crate) status_code: StatusCode,
    pub(crate) content_type: mime::Mime,
    pub(crate) content_disposition: String,
    pub(crate) content_encoding: Option<String>,
}

pub struct NamedFileBuilder {
    path: PathBuf,
    file: Option<File>,
    attached_filename: Option<String>,
    disposition_type: Option<String>,
    content_type: Option<mime::Mime>,
    content_encoding: Option<String>,
    content_disposition: Option<String>,
    buffer_size: Option<u64>,
}
impl NamedFileBuilder {
    pub fn with_attached_filename<T: Into<String>>(mut self, attached_filename: T) -> NamedFileBuilder {
        self.attached_filename = Some(attached_filename.into());
        self
    }
    pub fn with_disposition_type<T: Into<String>>(mut self, disposition_type: T) -> NamedFileBuilder {
        self.disposition_type = Some(disposition_type.into());
        self
    }
    pub fn with_content_type(mut self, content_type: mime::Mime) -> NamedFileBuilder {
        self.content_type = Some(content_type);
        self
    }
    pub fn with_content_encoding<T: Into<String>>(mut self, content_encoding: T) -> NamedFileBuilder {
        self.content_encoding = Some(content_encoding.into());
        self
    }
    pub fn with_buffer_size(mut self, buffer_size: u64) -> NamedFileBuilder {
        self.buffer_size = Some(buffer_size);
        self
    }
    pub fn build(mut self) -> io::Result<NamedFile> {
        if self.file.is_none() {
            self.file = Some(File::open(&self.path)?);
        }
        let ct = from_path(&self.path).first_or_octet_stream();
        if self.content_type.is_none() {
            self.content_type = Some(ct);
        }
        if self.disposition_type.is_none() {
            let disposition_type = if self.attached_filename.is_some() {
                "attachment"
            } else {
                match self.content_type.as_ref().unwrap().type_() {
                    mime::IMAGE | mime::TEXT | mime::VIDEO => "inline",
                    _ => "attachment",
                }
            };
            if disposition_type == "attachment" && self.attached_filename.is_none() {
                let filename = match self.path.file_name() {
                    Some(name) => name.to_string_lossy(),
                    None => {
                        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Provided path has no filename"));
                    }
                };
                self.attached_filename = Some(filename.into());
            }
            self.disposition_type = Some(disposition_type.into());
        }
        if let Some("attachment") = self.disposition_type.as_deref() {
            self.content_disposition = Some(format!(
                "{};filename=\"{}\"",
                self.disposition_type.as_ref().unwrap(),
                self.attached_filename.as_ref().unwrap()
            ));
        } else {
            self.content_disposition = Some("inline".into());
        }

        let metadata = self.file.as_ref().unwrap().metadata()?;
        let modified = metadata.modified().ok();

        let NamedFileBuilder {
            path,
            file,
            content_type,
            content_encoding,
            content_disposition,
            buffer_size,
            ..
        } = self;

        Ok(NamedFile {
            path,
            file: file.unwrap(),
            content_type: content_type.unwrap(),
            content_disposition: content_disposition.unwrap(),
            metadata,
            modified,
            content_encoding,
            buffer_size: buffer_size.unwrap_or(65_536),
            status_code: StatusCode::OK,
            flags: Flags::default(),
        })
    }
}

impl NamedFile {
    pub fn builder(path: PathBuf) -> NamedFileBuilder {
        NamedFileBuilder {
            path,
            file: None,
            attached_filename: None,
            disposition_type: None,
            content_type: None,
            content_encoding: None,
            content_disposition: None,
            buffer_size: None,
        }
    }

    /// Attempts to open a file in read-only mode.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use salvo_core::fs::NamedFile;
    /// let file = NamedFile::open("foo.txt".into());
    /// ```
    pub fn open(path: PathBuf) -> io::Result<NamedFile> {
        Self::builder(path).build()
    }

    /// Returns reference to the underlying `File` object.
    #[inline]
    pub fn file(&self) -> &File {
        &self.file
    }

    /// Retrieve the path of this file.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use std::io;
    /// # use salvo_core::fs::NamedFile;
    /// # fn path() -> io::Result<()> {
    /// let file = NamedFile::open("test.txt".into())?;
    /// assert_eq!(file.path().as_os_str(), "foo.txt");
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Set the MIME Content-Type for serving this file. By default
    /// the Content-Type is inferred from the filename extension.
    #[inline]
    pub fn set_content_type(mut self, mime_type: mime::Mime) -> Self {
        self.content_type = mime_type;
        self
    }

    /// Set the Content-Disposition for serving this file. This allows
    /// changing the inline/attachment disposition as well as the filename
    /// sent to the peer. By default the disposition is `inline` for text,
    /// image, and video content types, and `attachment` otherwise, and
    /// the filename is taken from the path provided in the `open` method
    /// after converting it to UTF-8 using.
    /// [to_string_lossy](https://doc.rust-lang.org/std/ffi/struct.OsStr.html#method.to_string_lossy).
    #[inline]
    pub fn set_content_disposition(mut self, cd: String) -> Self {
        self.content_disposition = cd;
        self.flags.insert(Flags::CONTENT_DISPOSITION);
        self
    }

    /// Disable `Content-Disposition` header.
    ///
    /// By default Content-Disposition` header is enabled.
    #[inline]
    pub fn disable_content_disposition(mut self) -> Self {
        self.flags.remove(Flags::CONTENT_DISPOSITION);
        self
    }

    /// Set content encoding for serving this file
    #[inline]
    pub fn set_content_encoding(mut self, enc: String) -> Self {
        self.content_encoding = Some(enc);
        self
    }

    #[inline]
    ///Specifies whether to use ETag or not.
    ///
    ///Default is true.
    pub fn use_etag(mut self, value: bool) -> Self {
        self.flags.set(Flags::ETAG, value);
        self
    }

    #[inline]
    ///Specifies whether to use Last-Modified or not.
    ///
    ///Default is true.
    pub fn use_last_modified(mut self, value: bool) -> Self {
        self.flags.set(Flags::LAST_MODIFIED, value);
        self
    }
    pub(crate) fn etag(&self) -> Option<ETag> {
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

            let dur = mtime.duration_since(UNIX_EPOCH).expect("modification time must be after epoch");
            let etag_str = format!("\"{:x}-{:x}-{:x}-{:x}\"", ino, self.metadata.len(), dur.as_secs(), dur.subsec_nanos());
            match etag_str.parse::<ETag>() {
                Ok(etag) => Some(etag),
                Err(e) => {
                    tracing::error!(error = ?e, etag = %etag_str, "set file's etag failed");
                    None
                }
            }
        })
    }

    pub(crate) fn last_modified(&self) -> Option<SystemTime> {
        self.modified
    }
}

#[async_trait]
impl Writer for NamedFile {
    async fn write(mut self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        let etag = if self.flags.contains(Flags::ETAG) { self.etag() } else { None };
        let last_modified = if self.flags.contains(Flags::LAST_MODIFIED) {
            self.last_modified()
        } else {
            None
        };

        // check preconditions
        let precondition_failed = if !any_match(etag.as_ref(), req) {
            true
        } else if let (Some(ref last_modified), Some(since)) = (last_modified, req.headers().typed_get::<IfUnmodifiedSince>()) {
            !since.precondition_passes(*last_modified)
        } else {
            false
        };

        // check last modified
        let not_modified = if !none_match(etag.as_ref(), req) {
            true
        } else if req.headers().contains_key(header::IF_NONE_MATCH) {
            false
        } else if let (Some(ref last_modified), Some(since)) = (last_modified, req.headers().typed_get::<IfModifiedSince>()) {
            !since.is_modified(*last_modified)
        } else {
            false
        };

        match self.content_disposition.parse::<HeaderValue>() {
            Ok(content_disposition) => {
                res.headers_mut().insert(CONTENT_DISPOSITION, content_disposition);
            }
            Err(e) => {
                tracing::error!(error = ?e, "set file's content disposition failed");
            }
        }
        res.headers_mut().typed_insert(ContentType::from(self.content_type.clone()));
        if let Some(lm) = last_modified {
            res.headers_mut().typed_insert(LastModified::from(lm));
        }
        if let Some(etag) = self.etag() {
            res.headers_mut().typed_insert(etag);
        }
        res.headers_mut().typed_insert(AcceptRanges::bytes());

        let mut length = self.metadata.len();
        if let Some(content_encoding) = &self.content_encoding {
            match content_encoding.parse::<HeaderValue>() {
                Ok(content_encoding) => {
                    res.headers_mut().insert(CONTENT_ENCODING, content_encoding);
                }
                Err(e) => {
                    tracing::error!(error = ?e, "set file's content encoding failed");
                }
            }
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
                file: self.file,
                buffer_size: self.buffer_size,
            };
            res.headers_mut().typed_insert(ContentLength(reader.chunk_size));
            res.streaming(reader)
        } else {
            res.set_status_code(StatusCode::OK);
            let reader = FileChunk {
                offset,
                file: self.file,
                chunk_size: length,
                read_size: 0,
                buffer_size: self.buffer_size,
            };
            res.headers_mut().typed_insert(ContentLength(length - offset));
            res.streaming(reader)
        }
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
