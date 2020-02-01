use std::io::Write;
use std::path::{PathBuf, Path};
use std::ops::Drop;
use textnonce::TextNonce;
use mime::Mime;
use http::header;
use hyper::body::HttpBody;
use url::form_urlencoded;
use multimap::MultiMap;
use tempdir::TempDir;
use futures::stream::TryStreamExt;
use futures::{Stream, TryStream};
use hyper::body::Bytes;

use crate::http::request::{self, Request};
use crate::http::errors::ReadError;
use crate::http::multipart::Multipart;
use crate::http::header::{HeaderValue, HeaderMap, CONTENT_DISPOSITION, CONTENT_TYPE};
use crate::http::{Body, BodyChunk};

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
            let mut multipart = Multipart::try_from_body_headers(body, headers)?;
            let mut form_data = FormData::new();
            while let Some(mut field) = multipart.next_field().await? {
                if field.headers.is_text() {
                    form_data.fields.insert(field.headers.name, field.data.read_to_string().await?);
                } else {
                    while let Some(chunk) = field.data.try_next().await? {
                        //println!("got field chunk, len: {:?}", chunk.len());
                    }
                }
            }
            Ok(form_data)
        },
        _ => Err(ReadError::Parsing("parse form data failed".into())),
    }
}
#[inline]
fn get_content_disposition_name(hv: &HeaderValue) -> Option<String> {
     for part in hv.to_str().unwrap_or("").split(';'){
        if part.trim().starts_with("name=") {
            return Some(part.trim().trim_start_matches("name=").to_owned());
        }
    }
    None
}

/// A file that is to be inserted into a `multipart/*` or alternatively an uploaded file that
/// was received as part of `multipart/*` parsing.
#[derive(Clone, Debug, PartialEq)]
pub struct FilePart {
    /// The headers of the part
    pub headers: HeaderMap,
    /// A temporary file containing the file content
    pub path: PathBuf,
    /// Optionally, the size of the file.  This is filled when multiparts are parsed, but is
    /// not necessary when they are generated.
    pub size: Option<usize>,
    // The temporary directory the upload was put into, saved for the Drop trait
    temp_dir: Option<PathBuf>,
}
impl FilePart {
    pub fn new(headers: HeaderMap, path: &Path) -> FilePart
    {
        FilePart {
            headers,
            path: path.to_owned(),
            size: None,
            temp_dir: None,
        }
    }

    /// If you do not want the file on disk to be deleted when Self drops, call this
    /// function.  It will become your responsability to clean up.
    pub fn do_not_delete_on_drop(&mut self) {
        self.temp_dir = None;
    }

    /// Create a new temporary FilePart (when created this way, the file will be
    /// deleted once the FilePart object goes out of scope).
    pub fn create(headers: HeaderMap) -> Result<FilePart, ReadError> {
        // Setup a file to capture the contents.
        let mut path = TempDir::new("novel_http_multipart")?.into_path();
        let temp_dir = Some(path.clone());
        path.push(TextNonce::sized_urlsafe(32).unwrap().into_string());
        Ok(FilePart {
            headers,
            path,
            size: None,
            temp_dir,
        })
    }

    pub fn filename(&self) -> Option<String> {
        self.headers.get(CONTENT_DISPOSITION).and_then(|cd|get_content_disposition_name(cd))
    }

    /// Mime content-type specified in the header
    pub fn content_type(&self) -> Option<Mime> {
        self.headers.get(CONTENT_TYPE).and_then(|hv|hv.to_str().ok()).and_then(|hv|hv.parse().ok())
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
/// The extracted text fields and uploaded files from a `multipart/form-data` request.
///
/// Use `parse_multipart` to devise this object from a request.
#[derive(Clone, Debug, PartialEq)]
pub struct FormData {
    /// Name-value pairs for plain text fields. Technically, these are form data parts with no
    /// filename specified in the part's `Content-Disposition`.
    pub fields: MultiMap<String, String>,
    /// Name-value pairs for temporary files. Technically, these are form data parts with a filename
    /// specified in the part's `Content-Disposition`.
    pub files: MultiMap<String, FilePart>,
}

impl FormData {
    pub fn new() -> FormData {
        FormData { fields: MultiMap::new(), files: MultiMap::new() }
    }
}
