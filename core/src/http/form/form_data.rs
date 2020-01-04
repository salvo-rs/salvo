use multimap::MultiMap;
use super::multipart::{Node, Part, FilePart};
use http::header::{HeaderMap, HeaderValue, CONTENT_DISPOSITION, CONTENT_TYPE};
use super::error::Error;

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
    pub fn to_multipart(&self) -> Result<Vec<Node>, Error> {
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
                    None => return Err(Error::NotAFile),
                };
                filepart.headers.append(CONTENT_DISPOSITION, HeaderValue::from_str(&format!("form-data; name={}; filename={}", key, filename)).unwrap());
                nodes.push( Node::File( filepart ) );
            }
        }

        Ok(nodes)
    }
}
