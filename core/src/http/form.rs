use crate::http::multipart;

use std::io::Write;
use crate::http::header::{HeaderValue, HeaderMap, CONTENT_DISPOSITION, CONTENT_TYPE};
use multipart::Node;
use std::ops::Drop;
use textnonce::TextNonce;
use mime::Mime;

use http::header;
use hyper::body::HttpBody;
use hyper::body::Body;
use url::form_urlencoded;
use crate::http::request;
use multimap::MultiMap;
use crate::http::errors::ReadError;

/// Parse MIME `multipart/form-data` information from a stream as a `FormData`.
pub async fn read_form_data(body: Body, headers: &HeaderMap) -> Result<FormData, ReadError> {
    match headers.get(header::CONTENT_TYPE) {
        Some(ctype) if ctype == "application/x-www-form-urlencoded" => {
            let data = request::read_body_bytes(body).await?;
            let mut form_data = FormData::new();
            form_data.fields = form_urlencoded::parse(data.as_ref()).into_owned().collect();
            Ok(form_data)
        },
        Some(ctype) if ctype.to_str().unwrap_or("").starts_with("multipart/form-data") => {
            let nodes = multipart::read_multipart_body(request::read_body_cursor(body).await?, headers, false)?;
        
            let mut form_data = FormData::new();
            fill_form_data(&mut form_data, nodes)?;
            Ok(form_data)
        },
        _ => Err(ReadError::General("parse form data failed".into())),
    }
}

// order and nesting are irrelevant, so we interate through the nodes and put them
// into one of two buckets (fields and files);  If a multipart node is found, it uses
// the name in its headers as the key (rather than the name in the headers of the
// subparts), which is how multiple file uploads work.
fn fill_form_data(form_data: &mut FormData, nodes: Vec<Node>) -> Result<(), ReadError> {
    for node in nodes {
        match node {
            Node::Part(part) => {
                let cd_name: Option<String> = part.headers.get(CONTENT_DISPOSITION).and_then(|hv|get_content_disposition_name(&hv));
                let key = cd_name.ok_or(ReadError::NoName)?;
                let val = String::from_utf8(part.body)?;
                form_data.fields.insert(key, val);
            },
            Node::File(part) => {
                /*let cd_name: Option<String> = {
                    let cd: &ContentDisposition = match part.headers.get() {
                        Some(cd) => cd,
                        None => return Err(Error::MissingDisposition),
                    };
                    get_content_disposition_name(&cd)
                };*/
                
                let cd_name: Option<String> = part.headers.get(CONTENT_DISPOSITION).and_then(|hv|get_content_disposition_name(&hv));
                let key = cd_name.ok_or(ReadError::NoName)?;
                form_data.files.insert(key, part);
            }
            Node::Multipart((headers, nodes)) => {
                /*let cd_name: Option<String> = {
                    let cd: &ContentDisposition = match headers.get() {
                        Some(cd) => cd,
                        None => return Err(Error::MissingDisposition),
                    };
                    get_content_disposition_name(&cd)
                };*/
                
                let cd_name: Option<String> = headers.get(CONTENT_DISPOSITION).and_then(|hv|get_content_disposition_name(&hv));
                let key = cd_name.ok_or(ReadError::NoName)?;
                for node in nodes {
                    match node {
                        Node::Part(part) => {
                            let val = String::from_utf8(part.body)?;
                            form_data.fields.insert(key.clone(), val);
                        },
                        Node::File(part) => {
                            form_data.files.insert(key.clone(), part);
                        },
                        _ => { } // don't recurse deeper
                    }
                }
            }
        }
    }
    Ok(())
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


/// Stream out `multipart/form-data` body content matching the passed in `form_data`.  This
/// does not stream out headers, so the caller must stream those out before calling
/// write_form_data().
pub fn write_form_data<S: Write>(stream: &mut S, boundary: &[u8], form_data: &FormData) -> Result<usize, ReadError> {
    //TODO
    let nodes = form_data.to_multipart()?;
    let count = multipart::write_multipart(stream, boundary, &nodes)?;
    Ok(count)
}

/// Stream out `multipart/form-data` body content matching the passed in `form_data` as
/// Transfer-Encoding: Chunked.  This does not strea m out headers, so the caller must stream
/// those out before calling write_form_data().
pub fn write_form_data_chunked<S: Write>(stream: &mut S, boundary: &[u8], form_data: &FormData)
                                        -> Result<(), ReadError>
{
    //TODO
    // let nodes = form_data.to_multipart()?;
    // multipart::write_multipart_chunked(stream, boundary, &nodes)?;
    Ok(())
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

    /// Filename that was specified when the file was uploaded.  Returns `Ok<None>` if there
    /// was no content-disposition header supplied.
    pub fn filename(&self) -> Result<Option<String>, ReadError> {
        match self.headers.get(CONTENT_DISPOSITION) {
            Some(cd) => get_content_disposition_filename(cd),
            None => Ok(None),
        }
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

    /// Create a mime-multipart Vec<Node> from this FormData
    pub fn to_multipart(&self) -> Result<Vec<Node>, ReadError> {
        // Translate to Nodes
        let mut nodes: Vec<Node> = Vec::with_capacity(self.fields.len() + self.files.len());

        for (key, values) in self.fields.iter_all() {
            for value in values {
                let mut h = HeaderMap::new();
                h.append(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
                h.append(CONTENT_DISPOSITION, HeaderValue::from_str(&format!("form-data; name={}", key)).unwrap());
                nodes.push( Node::Part( Part {
                    headers: h,
                    body: value.as_bytes().to_owned(),
                }));
            }
        }

        for (key, fileparts) in self.files.iter_all() {
            for filepart in fileparts {
                let mut filepart = filepart.clone();
                // We leave all headers that the caller specified, except that we rewrite
                // Content-Disposition.
                filepart.headers.remove(CONTENT_DISPOSITION);
                let filename = match filepart.path.file_name() {
                    Some(fname) => fname.to_string_lossy().into_owned(),
                    None => return Err(ReadError::NotAFile),
                };
                filepart.headers.append(CONTENT_DISPOSITION, HeaderValue::from_str(&format!("form-data; name={}; filename={}", key, filename)).unwrap());
                nodes.push( Node::File( filepart ) );
            }
        }

        Ok(nodes)
    }
}
