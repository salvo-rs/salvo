mod form_data;
mod multipart;

pub use form_data::FormData;

use std::io::Write;
use crate::http::header::{HeaderValue, HeaderMap, CONTENT_DISPOSITION};
use multipart::Node;
pub use multipart::FilePart;
pub use multipart::{read_multipart, generate_boundary};

use http::header;
use hyper::body::HttpBody;
use url::form_urlencoded;
use crate::http::errors::ReadError;
use crate::http::request;

/// Parse MIME `multipart/form-data` information from a stream as a `FormData`.
pub fn read_form_data<S: HttpBody>(body: S, headers: &HeaderMap) -> Result<FormData, ReadError> {
    match headers.get(header::CONTENT_TYPE) {
        Some(ctype) if ctype == "application/x-www-form-urlencoded" => {
            let data = request::read_body_bytes(body)?;
            let form_data = FormData::new();
            form_data.fields = form_urlencoded::parse(data.as_ref()).into_owned().collect();
            Ok(form_data)
        },
        Some(ctype) if ctype == "multipart/form-data" => {
            let nodes = multipart::read_multipart_body(request::read_body_cursor(body)?, headers, false)?;
        
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
    let nodes = form_data.to_multipart()?;
    multipart::write_multipart_chunked(stream, boundary, &nodes)?;
    Ok(())
}