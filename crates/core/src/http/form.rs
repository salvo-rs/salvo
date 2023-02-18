//! form parse module
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use http_body_util::BodyExt;
use multer::{Field, Multipart};
use multimap::MultiMap;
use tempfile::Builder;
use textnonce::TextNonce;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::http::body::ReqBody;
use crate::http::header::{HeaderMap, CONTENT_TYPE};
use crate::http::ParseError;

/// The extracted text fields and uploaded files from a `multipart/form-data` request.
#[derive(Debug)]
pub struct FormData {
    /// Name-value pairs for plain text fields. Technically, these are form data parts with no
    /// filename specified in the part's `Content-Disposition`.
    pub fields: MultiMap<String, String>,
    /// Name-value pairs for temporary files. Technically, these are form data parts with a filename
    /// specified in the part's `Content-Disposition`.
    pub files: MultiMap<String, FilePart>,
}

impl FormData {
    /// Create new `FormData`.
    #[inline]
    pub fn new() -> FormData {
        FormData {
            fields: MultiMap::new(),
            files: MultiMap::new(),
        }
    }

    /// Parse MIME `multipart/*` information from a stream as a [`FormData`].
    pub(crate) async fn read(headers: &HeaderMap, body: ReqBody) -> Result<FormData, ParseError> {
        match headers.get(CONTENT_TYPE) {
            Some(ctype) if ctype == "application/x-www-form-urlencoded" => {
                let data = BodyExt::collect(body).await.map_err(ParseError::other)?.to_bytes();
                let mut form_data = FormData::new();
                form_data.fields = form_urlencoded::parse(&data).into_owned().collect();
                Ok(form_data)
            }
            Some(ctype) if ctype.to_str().unwrap_or("").starts_with("multipart/") => {
                let mut form_data = FormData::new();
                if let Some(boundary) = headers
                    .get(CONTENT_TYPE)
                    .and_then(|ct| ct.to_str().ok())
                    .and_then(|ct| multer::parse_boundary(ct).ok())
                {
                    let mut multipart = Multipart::new(body, boundary);
                    while let Some(mut field) = multipart.next_field().await? {
                        if let Some(name) = field.name().map(|s| s.to_owned()) {
                            if field.headers().get(CONTENT_TYPE).is_some() {
                                form_data.files.insert(name, FilePart::create(&mut field).await?);
                            } else {
                                form_data.fields.insert(name, field.text().await?);
                            }
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
    size: Option<usize>,
    // The temporary directory the upload was put into, saved for the Drop trait
    temp_dir: Option<PathBuf>,
}
impl FilePart {
    /// Get file name.
    #[inline]
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
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    /// Get headers mutable reference.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }
    /// Get file path.
    #[inline]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
    /// Get file size.
    #[inline]
    pub fn size(&self) -> Option<usize> {
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
    #[inline]
    pub async fn create(field: &mut Field<'_>) -> Result<FilePart, ParseError> {
        // Setup a file to capture the contents.
        let mut path = tokio::task::spawn_blocking(|| Builder::new().prefix("salvo_http_multipart").tempdir())
            .await
            .expect("Runtime spawn blocking poll error")?
            .into_path();
        let temp_dir = Some(path.clone());
        let name = field.file_name().map(|s| s.to_owned());
        path.push(format!(
            "{}.{}",
            TextNonce::sized_urlsafe(32).unwrap().into_string(),
            name.as_deref()
                .and_then(|name| { Path::new(name).extension().and_then(OsStr::to_str) })
                .unwrap_or("unknown")
        ));
        let mut file = File::create(&path).await?;
        while let Some(chunk) = field.chunk().await? {
            file.write_all(&chunk).await?;
        }
        Ok(FilePart {
            name,
            headers: field.headers().to_owned(),
            path,
            size: None,
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
                std::fs::remove_file(&path).ok();
                std::fs::remove_dir(temp_dir).ok();
            });
        }
    }
}
