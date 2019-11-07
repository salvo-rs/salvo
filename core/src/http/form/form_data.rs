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
    pub fields: Vec<(String, String)>,
    /// Name-value pairs for temporary files. Technically, these are form data parts with a filename
    /// specified in the part's `Content-Disposition`.
    pub files: Vec<(String, FilePart)>,
}

impl FormData {
    pub fn new() -> FormData {
        FormData { fields: vec![], files: vec![] }
    }

    /// Create a mime-multipart Vec<Node> from this FormData
    pub fn to_multipart(&self) -> Result<Vec<Node>, Error> {
        // Translate to Nodes
        let mut nodes: Vec<Node> = Vec::with_capacity(self.fields.len() + self.files.len());

        for &(ref name, ref value) in &self.fields {
            let mut h = HeaderMap::new();
            h.append(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
            h.append(CONTENT_DISPOSITION, HeaderValue::from_str(&format!("form-data; name={}", name)).unwrap());
            nodes.push( Node::Part( Part {
                headers: h,
                body: value.as_bytes().to_owned(),
            }));
        }

        for &(ref name, ref filepart) in &self.files {
            let mut filepart = filepart.clone();
            // We leave all headers that the caller specified, except that we rewrite
            // Content-Disposition.
            filepart.headers.remove(CONTENT_DISPOSITION);
            let filename = match filepart.path.file_name() {
                Some(fname) => fname.to_string_lossy().into_owned(),
                None => return Err(Error::NotAFile),
            };
            filepart.headers.append(CONTENT_DISPOSITION, HeaderValue::from_str(&format!("form-data; name={}; filename={}", name, filename)).unwrap());
            nodes.push( Node::File( filepart ) );
        }

        Ok(nodes)
    }

    pub fn get_fields<T: AsRef<str>>(&self, key: T) -> Vec<&String>{
        let mut result: Vec<&String> = vec![];
        for (ref k, ref v) in &self.fields {
            if key.as_ref() == k {
                result.push(&v);
            }
        }
        result
    }
    pub fn get_one_field<T: AsRef<str>>(&self, key: T) -> Option<&String>{
        for (ref k, ref v) in &self.fields {
            if key.as_ref() == k {
                return Some(v);
            }
        }
        None
    }
}
