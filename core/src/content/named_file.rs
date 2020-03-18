use std::fs::{File, Metadata};
use std::{cmp, io};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::io::Read;
use async_trait::async_trait;
use httpdate::{self, HttpDate};
use std::io::Seek;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use bitflags::bitflags;
use mime_guess::from_path;

use crate::http::range::HttpRange;
use crate::http::header;
use crate::http::{StatusCode, Request, Response};
use crate::http::errors::*;
use super::Content;

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
    pub(crate) metadata: Metadata,
    pub(crate) flags: Flags,
    pub(crate) status_code: StatusCode,
    pub(crate) content_type: mime::Mime,
    pub(crate) content_disposition: String,
    pub(crate) encoding: Option<String>,
}

impl NamedFile {
    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<NamedFile> {
        let file = File::open(path.as_ref())?;
        Self::from_file(file, path)
    }
    pub fn from_file<P: AsRef<Path>>(file: File, path: P) -> io::Result<NamedFile> {
        let path = path.as_ref().to_path_buf();

        // Get the name of the file and use it to construct default Content-Type
        // and Content-Disposition values
        let (content_type, content_disposition) = {
            let filename = match path.file_name() {
                Some(name) => name.to_string_lossy(),
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Provided path has no filename",
                    ));
                }
            };

            let ct = from_path(&path).first_or_octet_stream();
            let disposition_type = match ct.type_() {
                mime::IMAGE | mime::TEXT | mime::VIDEO => "inline",
                _ => "attachment",
            };
            (ct, format!("{};filename=\"{}\"", disposition_type, filename.as_ref()))
        };

        let metadata = file.metadata()?;
        let modified = metadata.modified().ok();
        let encoding = None;
        Ok(NamedFile {
            path,
            file,
            content_type,
            content_disposition,
            metadata,
            modified,
            encoding,
            status_code: StatusCode::OK,
            flags: Flags::default(),
        })
    }

    /// Attempts to open a file in read-only mode.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use actix_files::NamedFile;
    ///
    /// let file = NamedFile::open("foo.txt");
    /// ```
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<NamedFile> {
        Self::from_file(File::open(&path)?, path)
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
    /// use actix_files::NamedFile;
    ///
    /// # fn path() -> io::Result<()> {
    /// let file = NamedFile::open("test.txt")?;
    /// assert_eq!(file.path().as_os_str(), "foo.txt");
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Set response **Status Code**
    pub fn set_status_code(mut self, status: StatusCode) -> Self {
        self.status_code = status;
        self
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
        self.encoding = Some(enc);
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

    pub(crate) fn etag(&self) -> Option<String> {
        // This etag format is similar to Apache's.
        self.modified.as_ref().map(|mtime| {
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
            format!(
                "{:x}:{:x}:{:x}:{:x}",
                ino,
                self.metadata.len(),
                dur.as_secs(),
                dur.subsec_nanos()
            )
        })
    }

    pub(crate) fn last_modified(&self) -> Option<HttpDate> {
        self.modified.map(|mtime| mtime.into())
    }
}

#[async_trait]
impl Content for NamedFile {
    async fn apply(mut self, req: &mut Request, resp: &mut Response) {
        if self.status_code != StatusCode::OK {
            resp.set_status_code(self.status_code);
            resp.set_content_disposition(&self.content_disposition);
            if let Some(current_encoding) = &self.encoding {
                resp.set_content_encoding(current_encoding);
            }
            match read_file_bytes(&mut self.file, self.metadata.len(), 0, 0) {
                Ok(data) => {
                    resp.render(self.content_type.to_string(), data);
                },
                Err(_) => {
                    resp.set_http_error(InternalServerError::new("file read error", "can not read this file"));
                },
            }
            return
        }

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
        let precondition_failed = if !any_match(etag.as_deref(), req) {
            true
        } else if let (Some(ref m), Some(since)) =
            (last_modified, req.get_header(header::IF_UNMODIFIED_SINCE))
        {
            let t1: SystemTime = m.clone().into();
            if let Ok(since) = since.to_str() {
                let t2: SystemTime = httpdate::parse_http_date(since).unwrap_or(SystemTime::now());
                match (t1.duration_since(UNIX_EPOCH), t2.duration_since(UNIX_EPOCH)) {
                    (Ok(t1), Ok(t2)) => t1 > t2,
                    _ => false,
                }
            } else {
                false
            }
        } else {
            false
        };

        // check last modified
        let not_modified = if !none_match(etag.as_deref(), req) {
            true
        } else if req.headers().contains_key(&header::IF_NONE_MATCH) {
            false
        } else if let (Some(ref m), Some(since)) =
            (last_modified, req.get_header(header::IF_MODIFIED_SINCE))
        {
            let t1: SystemTime = m.clone().into();
            if let Ok(since) = since.to_str() {
                let t2: SystemTime = httpdate::parse_http_date(since).unwrap_or(SystemTime::now());
                match (t1.duration_since(UNIX_EPOCH), t2.duration_since(UNIX_EPOCH)) {
                    (Ok(t1), Ok(t2)) => t1 <= t2,
                    _ => false,
                }
            } else {
                false
            }
        } else {
            false
        };

        resp.set_content_disposition(&self.content_disposition);
        // default compressing
        if let Some(current_encoding) = &self.encoding {
            resp.set_content_encoding(current_encoding);
        }

        if let Some(lm) = last_modified {
            resp.set_last_modified(lm);
        }
        if let Some(etag) = &etag {
            resp.set_etag(&etag);
        }
        resp.set_accept_range("bytes".into());

        let mut length = self.metadata.len();
        let mut offset = 0;

        // check for range header
        if let Some(ranges) = req.headers().get(&header::RANGE) {
            if let Ok(rangesheader) = ranges.to_str() {
                if let Ok(rangesvec) = HttpRange::parse(rangesheader, length) {
                    length = rangesvec[0].length;
                    offset = rangesvec[0].start;
                    resp.set_content_encoding("identity".into());
                    resp.set_content_range(&format!(
                            "bytes {}-{}/{}",
                            offset,
                            offset + length - 1,
                            self.metadata.len()
                        ));
                } else {
                    resp.set_content_range(&format!("bytes */{}", length));
                    resp.set_status_code(StatusCode::RANGE_NOT_SATISFIABLE);
                    return;
                };
            } else {
                resp.set_status_code(StatusCode::BAD_REQUEST);
                return;
            };
        };

        if precondition_failed {
            resp.set_status_code(StatusCode::PRECONDITION_FAILED);
            return
        } else if not_modified {
            resp.set_status_code(StatusCode::NOT_MODIFIED);
            return
        }

        if offset != 0 || length != self.metadata.len() {
            resp.set_status_code(StatusCode::PARTIAL_CONTENT);
            match read_file_bytes(&mut self.file, length, offset, 0) {
                Ok(data) => resp.render(self.content_type.to_string(), data),
                Err(_) => {
                    resp.set_http_error(InternalServerError::new("file read error", "can not read this file"));
                },
            }
        } else {
            resp.set_status_code(StatusCode::OK);
            let mut data = Vec::with_capacity(length as usize);
            match self.file.read_to_end(&mut data) {
                Ok(_) => resp.render(self.content_type.to_string(), data),
                Err(_) => {
                    resp.set_http_error(InternalServerError::new("file read error", "can not read this file"));
                },
            }
            
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
fn any_match(etag: Option<&str>, req: &Request) -> bool {
    match req.get_header(header::IF_MATCH).and_then(|v|v.to_str().ok()) {
        None | Some("any") => true,
        _ => {
            if let Some(some_etag) = etag {
                for item in req.headers().get_all(header::IF_MATCH) {
                    if let Ok(item) = item.to_str() {
                        if item == some_etag {
                            return true;
                        }
                    }
                }
            }
            false
        }
    }
}

/// Returns true if `req` doesn't have an `If-None-Match` header matching `req`.
fn none_match(etag: Option<&str>, req: &Request) -> bool {
    match req.get_header(header::IF_MATCH).and_then(|v|v.to_str().ok()) {
        Some("any") => false,
        None => true,
        _ => {
            if let Some(some_etag) = etag {
                for item in req.headers().get_all(header::IF_MATCH) {
                    if let Ok(item) = item.to_str() {
                        if item == some_etag {
                            return false;
                        }
                    }
                }
            }
            true
        }
    }
}

fn read_file_bytes(file: &mut File, size: u64, offset: u64, counter: u64) -> Result<Vec<u8>, io::Error> {
    let max_bytes: usize;
    max_bytes = cmp::min(size.saturating_sub(counter), 65_536) as usize;
    let mut buf = Vec::with_capacity(max_bytes);
    file.seek(io::SeekFrom::Start(offset))?;
    let nbytes =
        file.by_ref().take(max_bytes as u64).read_to_end(&mut buf)?;
    if nbytes == 0 {
        return Err(std::io::ErrorKind::UnexpectedEof.into());
    }
    Ok(buf)
}