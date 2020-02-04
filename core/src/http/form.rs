use std::fs::File;
use std::io::prelude::*;
use std::path::{PathBuf, Path};
use std::ops::Drop;
use textnonce::TextNonce;
use mime::Mime;
use http::header;
use url::form_urlencoded;
use multimap::MultiMap;
use tempdir::TempDir;
use futures::stream::TryStreamExt;
use std::ffi::OsStr;

use crate::http::request;
use crate::http::errors::ReadError;
use crate::http::multipart::{Multipart, Field, FieldHeaders};
use crate::http::header::HeaderMap;
use crate::http::{Body, BodyChunk};

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
    pub multipart: Option<Multipart<Body>>,
}

impl FormData {
    pub fn new() -> FormData {
        FormData { fields: MultiMap::new(), files: MultiMap::new(), multipart: None }
    }
}
fn get_extension_from_filename(filename: &str) -> Option<&str> {
    Path::new(filename)
        .extension()
        .and_then(OsStr::to_str)
}
/// A file that is to be inserted into a `multipart/*` or alternatively an uploaded file that
/// was received as part of `multipart/*` parsing.
#[derive(Debug)]
pub struct FilePart {
    /// The headers of the part
    pub headers: FieldHeaders,
    /// A temporary file containing the file content
    pub path: PathBuf,
    /// Optionally, the size of the file.  This is filled when multiparts are parsed, but is
    /// not necessary when they are generated.
    pub size: Option<usize>,
    // The temporary directory the upload was put into, saved for the Drop trait
    temp_dir: Option<PathBuf>,
}
impl FilePart {
    /// If you do not want the file on disk to be deleted when Self drops, call this
    /// function.  It will become your responsability to clean up.
    pub fn do_not_delete_on_drop(&mut self) {
        self.temp_dir = None;
    }

    /// Create a new temporary FilePart (when created this way, the file will be
    /// deleted once the FilePart object goes out of scope).
    pub async fn create(field: &mut Field<'_, Body>) -> Result<FilePart, ReadError> {
        // Setup a file to capture the contents.
        let mut path = TempDir::new("salvo_http_multipart")?.into_path();
        let temp_dir = Some(path.clone());
        path.push(format!("{}.{}", TextNonce::sized_urlsafe(32).unwrap().into_string(), 
            field.headers.filename.as_ref().and_then(|f|get_extension_from_filename(&f)).unwrap_or("unknown")));
        let mut file = File::create(&path)?;
        while let Some(chunk) = field.data.try_next().await? {
            file.write_all(chunk.as_slice())?;
        }
        Ok(FilePart {
            headers: field.headers.clone(),
            path,
            size: None,
            temp_dir,
        })
    }

    pub fn filename(&self) -> Option<&str> {
        self.headers.filename.as_deref()
    }

    /// Mime content-type specified in the header
    pub fn content_type(&self) -> Option<&Mime> {
        self.headers.content_type.as_ref()
    }
}
impl Drop for FilePart {
    fn drop(&mut self) {
        if self.temp_dir.is_some() {
            let _ = ::std::fs::remove_file(&self.path);
            let _ = ::std::fs::remove_dir(&self.temp_dir.as_ref().unwrap());
        }
    }
}

/// Parse MIME `multipart/form-data` information from a stream as a `FormData`.
pub async fn read_form_data(headers: &HeaderMap, body: Body) -> Result<FormData, ReadError> {
    match headers.get(header::CONTENT_TYPE) {
        Some(ctype) if ctype == "application/x-www-form-urlencoded" => {
            let data = request::read_body_bytes(body).await?;
            let mut form_data = FormData::new();
            form_data.fields = form_urlencoded::parse(data.as_ref()).into_owned().collect();
            Ok(form_data)
        },
        Some(ctype) if ctype.to_str().unwrap_or("").starts_with("multipart/form-data") => {
            let mut form_data = FormData::new();
            let mut multipart = Multipart::try_from_body_headers(body, headers)?;
            while let Some(mut field) = multipart.next_field().await? {
                if field.headers.is_text() {
                    form_data.fields.insert(field.headers.name.clone(), field.data.read_to_string().await?);
                } else {
                    form_data.files.insert(field.headers.name.clone(), FilePart::create(&mut field).await?);
                }
            }
            Ok(form_data)
        },
        _ => Err(ReadError::Parsing("parse form data failed".into())),
    }
}