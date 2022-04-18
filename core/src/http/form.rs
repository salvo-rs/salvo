//! form

use multer::{Field, Multipart};
use multimap::MultiMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempdir::TempDir;
use textnonce::TextNonce;
use tokio::{fs::File, io::AsyncWriteExt};

use crate::http::errors::ParseError;
use crate::http::header::{HeaderMap, CONTENT_TYPE};
use crate::http::request::{self, Body};

/// The extracted text fields and uploaded files from a `multipart/form-data` request.
///
/// Use `parse_multipart` to devise this object from a request.
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
    pub fn new() -> FormData {
        FormData {
            fields: MultiMap::new(),
            files: MultiMap::new(),
        }
    }
}
impl Default for FormData {
    fn default() -> Self {
        Self::new()
    }
}
fn get_extension_from_filename(filename: &str) -> Option<&str> {
    Path::new(filename).extension().and_then(OsStr::to_str)
}
/// A file that is to be inserted into a `multipart/*` or alternatively an uploaded file that
/// was received as part of `multipart/*` parsing.
#[derive(Debug)]
pub struct FilePart {
    file_name: Option<String>,
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
    pub fn file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }
    /// Get file name mutable reference.
    pub fn file_name_mut(&mut self) -> Option<&mut String> {
        self.file_name.as_mut()
    }
    /// Get headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    /// Get headers mutable reference.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }
    /// Get file path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
    /// Get file size.
    pub fn size(&self) -> Option<usize> {
        self.size
    }
    /// If you do not want the file on disk to be deleted when Self drops, call this
    /// function.  It will become your responsibility to clean up.
    pub fn do_not_delete_on_drop(&mut self) {
        self.temp_dir = None;
    }

    /// Create a new temporary FilePart (when created this way, the file will be
    /// deleted once the FilePart object goes out of scope).
    pub async fn create(field: &mut Field<'_>) -> Result<FilePart, ParseError> {
        // Setup a file to capture the contents.
        let mut path = tokio::task::spawn_blocking(|| TempDir::new("salvo_http_multipart"))
            .await
            .expect("Runtime spawn blocking poll error")?
            .into_path();
        let temp_dir = Some(path.clone());
        let file_name = field.file_name().map(|s| s.to_owned());
        path.push(format!(
            "{}.{}",
            TextNonce::sized_urlsafe(32).unwrap().into_string(),
            file_name
                .as_deref()
                .and_then(get_extension_from_filename)
                .unwrap_or("unknown")
        ));
        let mut file = File::create(&path).await?;
        while let Some(chunk) = field.chunk().await? {
            file.write_all(&chunk).await?;
        }
        Ok(FilePart {
            file_name,
            headers: field.headers().to_owned(),
            path,
            size: None,
            temp_dir,
        })
    }
}
impl Drop for FilePart {
    fn drop(&mut self) {
        if self.temp_dir.is_some() {
            let path = self.path.clone();
            let temp_dir = self.temp_dir.clone();
            tokio::task::spawn_blocking(move || {
                let _ = ::std::fs::remove_file(&path);
                let _ = ::std::fs::remove_dir(temp_dir.as_ref().unwrap());
            });
        }
    }
}

/// Parse MIME `multipart/*` information from a stream as a `FormData`.
pub(crate) async fn read_form_data(headers: &HeaderMap, body: Body) -> Result<FormData, ParseError> {
    match headers.get(CONTENT_TYPE) {
        Some(ctype) if ctype == "application/x-www-form-urlencoded" => {
            let data = request::read_body_bytes(body).await?;
            let mut form_data = FormData::new();
            form_data.fields = form_urlencoded::parse(data.as_ref()).into_owned().collect();
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
