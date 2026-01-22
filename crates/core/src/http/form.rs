//! Form parse module.
use std::error::Error as StdError;
use std::ffi::OsStr;
use std::io::{Cursor, Error as IoError, ErrorKind, Write};
use std::path::{Path, PathBuf};

use base64::engine::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use mime::Mime;
use multer::{Field, Multipart};
use multimap::MultiMap;
use rand::TryRngCore;
use rand::rngs::OsRng;
use tempfile::Builder;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::http::ParseError;
use crate::http::body::ReqBody;
use crate::http::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderMap};

#[derive(Debug)]
struct PayloadTooLargeError {
    max_size: usize,
}

impl std::fmt::Display for PayloadTooLargeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "payload too large (limit: {} bytes)", self.max_size)
    }
}

impl StdError for PayloadTooLargeError {}

fn payload_too_large_io(max_size: usize) -> IoError {
    IoError::new(ErrorKind::Other, PayloadTooLargeError { max_size })
}

fn is_payload_too_large_error(error: &multer::Error) -> bool {
    let mut source = error.source();
    while let Some(err) = source {
        if err.downcast_ref::<PayloadTooLargeError>().is_some() {
            return true;
        }
        if let Some(io_err) = err.downcast_ref::<IoError>() {
            if let Some(inner) = io_err.get_ref() {
                if inner.downcast_ref::<PayloadTooLargeError>().is_some() {
                    return true;
                }
            }
        }
        source = err.source();
    }
    false
}

fn map_multer_error(error: multer::Error, max_size: usize) -> ParseError {
    if is_payload_too_large_error(&error) {
        ParseError::PayloadTooLarge { max_size }
    } else {
        ParseError::Multer(error)
    }
}

fn content_length_exceeds(headers: &HeaderMap, max_size: usize) -> bool {
    let Some(value) = headers.get(CONTENT_LENGTH) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Ok(length) = value.parse::<u64>() else {
        return false;
    };
    length as u128 > max_size as u128
}

async fn collect_with_max_size(mut body: ReqBody, max_size: usize) -> Result<Bytes, ParseError> {
    let mut buf = BytesMut::new();
    let mut total = 0usize;
    while let Some(frame) = body.next().await {
        let frame = frame.map_err(ParseError::Io)?;
        let data = frame.into_data().unwrap_or_default();
        total = total.saturating_add(data.len());
        if total > max_size {
            return Err(ParseError::PayloadTooLarge { max_size });
        }
        buf.extend_from_slice(&data);
    }
    Ok(buf.freeze())
}

fn limit_stream(
    body: ReqBody,
    max_size: usize,
) -> impl futures_util::Stream<Item = Result<Bytes, IoError>> {
    let mut remaining = max_size;
    body.map(move |frame_result| {
        frame_result.and_then(|frame| {
            let data = frame.into_data().unwrap_or_default();
            let len = data.len();
            if len > remaining {
                return Err(payload_too_large_io(max_size));
            }
            remaining -= len;
            Ok(data)
        })
    })
}

/// The extracted text fields and uploaded files from a `multipart/form-data` request.
#[derive(Debug)]
#[non_exhaustive]
pub struct FormData {
    /// Name-value pairs for plain text fields. Technically, these are form data parts with no
    /// filename specified in the part's `Content-Disposition`.
    pub fields: MultiMap<String, String>,
    /// Name-value pairs for temporary files. Technically, these are form data parts with a
    /// filename specified in the part's `Content-Disposition`.
    pub files: MultiMap<String, FilePart>,
}

impl FormData {
    /// Create new `FormData`.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            fields: MultiMap::new(),
            files: MultiMap::new(),
        }
    }

    /// Parse MIME `multipart/*` information from a stream as a `FormData` with size limit.
    pub(crate) async fn read_with_max_size(
        headers: &HeaderMap,
        body: ReqBody,
        max_size: usize,
    ) -> Result<Self, ParseError> {
        if content_length_exceeds(headers, max_size) {
            return Err(ParseError::PayloadTooLarge { max_size });
        }
        let ctype: Option<Mime> = headers
            .get(CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse().ok());
        match ctype {
            Some(ctype) if ctype.subtype() == mime::WWW_FORM_URLENCODED => {
                let data = collect_with_max_size(body, max_size).await?;
                let mut form_data = Self::new();
                form_data.fields = form_urlencoded::parse(&data).into_owned().collect();
                Ok(form_data)
            }
            Some(ctype) if ctype.type_() == mime::MULTIPART => {
                let mut form_data = Self::new();
                if let Some(boundary) = headers
                    .get(CONTENT_TYPE)
                    .and_then(|ct| ct.to_str().ok())
                    .and_then(|ct| multer::parse_boundary(ct).ok())
                {
                    let body = limit_stream(body, max_size);
                    let mut multipart = Multipart::new(body, boundary);
                    loop {
                        match multipart.next_field().await {
                            Ok(Some(mut field)) => {
                                if let Some(name) = field.name().map(|s| s.to_owned()) {
                                    if field.headers().get(CONTENT_TYPE).is_some() {
                                        form_data
                                            .files
                                            .insert(name, FilePart::create(&mut field).await?);
                                    } else {
                                        form_data.fields.insert(name, field.text().await?);
                                    }
                                }
                            }
                            Ok(None) => break,
                            Err(error) => return Err(map_multer_error(error, max_size)),
                        }
                    }
                }
                Ok(form_data)
            }
            _ => Err(ParseError::InvalidContentType),
        }
    }
}
impl Default for FormData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
/// A file that is to be inserted into a `multipart/*` or alternatively an uploaded file that
/// was received as part of `multipart/*` parsing.
#[derive(Clone, Debug)]
pub struct FilePart {
    name: Option<String>,
    /// The headers of the part
    headers: HeaderMap,
    /// A temporary file containing the file content
    path: PathBuf,
    /// Optionally, the size of the file.  This is filled when multiparts are parsed, but is
    /// not necessary when they are generated.
    size: u64,
    // The temporary directory the upload was put into, saved for the Drop trait
    temp_dir: Option<PathBuf>,
}
impl FilePart {
    /// Get file name.
    #[inline]
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    /// Get file name mutable reference.
    #[inline]
    pub fn name_mut(&mut self) -> Option<&mut String> {
        self.name.as_mut()
    }
    /// Get headers.
    #[inline]
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    /// Get headers mutable reference.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }
    /// Get content type.
    #[inline]
    pub fn content_type(&self) -> Option<Mime> {
        self.headers
            .get(CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse().ok())
    }
    /// Get file path.
    #[inline]
    #[must_use]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
    /// Get file size.
    #[inline]
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }
    /// If you do not want the file on disk to be deleted when Self drops, call this
    /// function.  It will become your responsibility to clean up.
    #[inline]
    pub fn do_not_delete_on_drop(&mut self) {
        self.temp_dir = None;
    }

    /// Create a new temporary FilePart (when created this way, the file will be
    /// deleted once the FilePart object goes out of scope).
    pub async fn create(field: &mut Field<'_>) -> Result<Self, ParseError> {
        // Setup a file to capture the contents.
        let mut path =
            tokio::task::spawn_blocking(|| Builder::new().prefix("salvo_http_multipart").tempdir())
                .await
                .expect("Runtime spawn blocking poll error")?
                .keep();
        let temp_dir = Some(path.clone());
        let name = field.file_name().map(|s| {
            // Sanitize filename by removing invalid characters
            s.chars()
                .filter(|c| {
                    !matches!(
                        c,
                        '/' | '\\' | '\0' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
                    )
                })
                .collect::<String>()
        });
        path.push(format!(
            "{}.{}",
            text_nonce(),
            name.as_deref()
                .and_then(|name| { Path::new(name).extension().and_then(OsStr::to_str) })
                .unwrap_or("unknown")
        ));
        let mut file = File::create(&path).await?;
        let mut size = 0;
        while let Some(chunk) = field.chunk().await? {
            size += chunk.len() as u64;
            file.write_all(&chunk).await?;
        }
        file.sync_all().await?;
        Ok(Self {
            name,
            headers: field.headers().to_owned(),
            path,
            size,
            temp_dir,
        })
    }
}
impl Drop for FilePart {
    fn drop(&mut self) {
        if let Some(temp_dir) = &self.temp_dir {
            let path = self.path.clone();
            let temp_dir = temp_dir.to_owned();
            tokio::task::spawn_blocking(move || {
                let _ = std::fs::remove_file(&path);
                let _ = std::fs::remove_dir(temp_dir);
            });
        }
    }
}

// Port from https://github.com/mikedilger/textnonce/blob/master/src/lib.rs
fn text_nonce() -> String {
    const BYTE_LEN: usize = 24;
    let mut raw: Vec<u8> = vec![0; BYTE_LEN];

    // Get the first 12 bytes from the current time
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        let secs: u64 = now.as_secs();
        let nsecs: u32 = now.subsec_nanos();

        let mut cursor = Cursor::new(&mut *raw);
        Write::write_all(&mut cursor, &nsecs.to_le_bytes()).expect("write_all failed");
        Write::write_all(&mut cursor, &secs.to_le_bytes()).expect("write_all failed");

        // Get the last bytes from random data
        OsRng
            .try_fill_bytes(&mut raw[12..BYTE_LEN])
            .expect("OsRng.try_fill_bytes failed");
    } else {
        OsRng
            .try_fill_bytes(&mut raw[..])
            .expect("OsRng.try_fill_bytes failed");
    }

    // base64 encode
    URL_SAFE_NO_PAD.encode(&raw)
}
