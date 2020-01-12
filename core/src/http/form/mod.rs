pub mod error;
mod form_data;
mod multipart;
#[cfg(test)]
mod mock;

pub use error::Error;
pub use form_data::FormData;

use std::io::{Read, Write};
use crate::http::headers::{HeaderValue, HeaderMap, CONTENT_DISPOSITION};
use multipart::Node;
pub use multipart::FilePart;
pub use multipart::{read_multipart, generate_boundary};

/// Parse MIME `multipart/form-data` information from a stream as a `FormData`.
pub fn read_form_data<S: Read>(stream: &mut S, headers: &HeaderMap) -> Result<FormData, Error> {
    let nodes = multipart::read_multipart_body(stream, headers, false)?;

    let mut form_data = FormData::new();
    fill_form_data(&mut form_data, nodes)?;
    Ok(form_data)
}

// order and nesting are irrelevant, so we interate through the nodes and put them
// into one of two buckets (fields and files);  If a multipart node is found, it uses
// the name in its headers as the key (rather than the name in the headers of the
// subparts), which is how multiple file uploads work.
fn fill_form_data(form_data: &mut FormData, nodes: Vec<Node>) -> Result<(), Error> {
    for node in nodes {
        match node {
            Node::Part(part) => {
                let cd_name: Option<String> = part.headers.get(CONTENT_DISPOSITION).and_then(|hv|get_content_disposition_name(&hv));
                let key = cd_name.ok_or(Error::NoName)?;
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
                let key = cd_name.ok_or(Error::NoName)?;
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
                let key = cd_name.ok_or(Error::NoName)?;
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
pub fn write_form_data<S: Write>(stream: &mut S, boundary: &[u8], form_data: &FormData) -> Result<usize, Error> {
    let nodes = form_data.to_multipart()?;
    let count = multipart::write_multipart(stream, boundary, &nodes)?;
    Ok(count)
}

/// Stream out `multipart/form-data` body content matching the passed in `form_data` as
/// Transfer-Encoding: Chunked.  This does not strea m out headers, so the caller must stream
/// those out before calling write_form_data().
pub fn write_form_data_chunked<S: Write>(stream: &mut S, boundary: &[u8], form_data: &FormData)
                                        -> Result<(), Error>
{
    let nodes = form_data.to_multipart()?;
    multipart::write_multipart_chunked(stream, boundary, &nodes)?;
    Ok(())
}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::{FormData, read_form_data, write_form_data, write_form_data_chunked,
                FilePart, generate_boundary};

    use std::net::SocketAddr;
    use std::fs::File;
    use std::io::Write;

    use hyper::buffer::BufReader;
    use hyper::net::NetworkStream;
    use hyper::server::Request as HyperRequest;
    use http::header::{Headers, ContentDisposition, DispositionParam, ContentType,
                        DispositionType};
    use mime::{Mime, TopLevel, SubLevel};

    use mock::MockStream;

    #[test]
    fn parser() {
        let input = b"POST / HTTP/1.1\r\n\
                      Host: example.domain\r\n\
                      Content-Type: multipart/form-data; boundary=\"abcdefg\"\r\n\
                      Content-Length: 1000\r\n\
                      \r\n\
                      --abcdefg\r\n\
                      Content-Disposition: form-data; name=\"field1\"\r\n\
                      \r\n\
                      data1\r\n\
                      --abcdefg\r\n\
                      Content-Disposition: form-data; name=\"field2\"; filename=\"image.gif\"\r\n\
                      Content-Type: image/gif\r\n\
                      \r\n\
                      This is a file\r\n\
                      with two lines\r\n\
                      --abcdefg\r\n\
                      Content-Disposition: form-data; name=\"field3\"; filename=\"file.txt\"\r\n\
                      \r\n\
                      This is a file\r\n\
                      --abcdefg--";

        let mut mock = MockStream::with_input(input);

        let mock: &mut NetworkStream = &mut mock;
        let mut stream = BufReader::new(mock);
        let sock: SocketAddr = "127.0.0.1:80".parse().unwrap();
        let req = HyperRequest::new(&mut stream, sock).unwrap();
        let (_, _, headers, _, _, mut reader) = req.deconstruct();

        match read_form_data(&mut reader, &headers) {
            Ok(form_data) => {
                assert_eq!(form_data.fields.len(), 1);
                for (key, val) in form_data.fields {
                    if &key == "field1" {
                        assert_eq!(&val, "data1");
                    }
                }

                assert_eq!(form_data.files.len(), 2);
                for (key, file) in form_data.files {
                    if &key == "field2" {
                        assert_eq!(file.size, Some(30));
                        assert_eq!(&*file.filename().unwrap().unwrap(), "image.gif");
                        assert_eq!(file.content_type().unwrap(), mime!(Image/Gif));
                    } else if &key == "field3" {
                        assert_eq!(file.size, Some(14));
                        assert_eq!(&*file.filename().unwrap().unwrap(), "file.txt");
                        assert!(file.content_type().is_none());
                    }
                }
            },
            Err(err) => panic!("{}", err),
        }
    }

    #[test]
    fn multi_file_parser() {
        let input = b"POST / HTTP/1.1\r\n\
                      Host: example.domain\r\n\
                      Content-Type: multipart/form-data; boundary=\"abcdefg\"\r\n\
                      Content-Length: 1000\r\n\
                      \r\n\
                      --abcdefg\r\n\
                      Content-Disposition: form-data; name=\"field1\"\r\n\
                      \r\n\
                      data1\r\n\
                      --abcdefg\r\n\
                      Content-Disposition: form-data; name=\"field2\"; filename=\"image.gif\"\r\n\
                      Content-Type: image/gif\r\n\
                      \r\n\
                      This is a file\r\n\
                      with two lines\r\n\
                      --abcdefg\r\n\
                      Content-Disposition: form-data; name=\"field2\"; filename=\"file.txt\"\r\n\
                      \r\n\
                      This is a file\r\n\
                      --abcdefg--";

        let mut mock = MockStream::with_input(input);

        let mock: &mut NetworkStream = &mut mock;
        let mut stream = BufReader::new(mock);
        let sock: SocketAddr = "127.0.0.1:80".parse().unwrap();
        let req = HyperRequest::new(&mut stream, sock).unwrap();
        let (_, _, headers, _, _, mut reader) = req.deconstruct();

        match read_form_data(&mut reader, &headers) {
            Ok(form_data) => {
                assert_eq!(form_data.fields.len(), 1);
                for (key, val) in form_data.fields {
                    if &key == "field1" {
                        assert_eq!(&val, "data1");
                    }
                }

                assert_eq!(form_data.files.len(), 2);
                let (ref key, ref file) = form_data.files[0];

                assert_eq!(key, "field2");
                assert_eq!(file.size, Some(30));
                assert_eq!(&*file.filename().unwrap().unwrap(), "image.gif");
                assert_eq!(file.content_type().unwrap(), mime!(Image/Gif));

                let (ref key, ref file) = form_data.files[1];
                assert!(key == "field2");
                assert_eq!(file.size, Some(14));
                assert_eq!(&*file.filename().unwrap().unwrap(), "file.txt");
                assert!(file.content_type().is_none());

            },
            Err(err) => panic!("{}", err),
        }
    }

    #[test]
    fn mixed_parser() {
        let input = b"POST / HTTP/1.1\r\n\
                      Host: example.domain\r\n\
                      Content-Type: multipart/form-data; boundary=AaB03x\r\n\
                      Content-Length: 1000\r\n\
                      \r\n\
                      --AaB03x\r\n\
                      Content-Disposition: form-data; name=\"submit-name\"\r\n\
                      \r\n\
                      Larry\r\n\
                      --AaB03x\r\n\
                      Content-Disposition: form-data; name=\"files\"\r\n\
                      Content-Type: multipart/mixed; boundary=BbC04y\r\n\
                      \r\n\
                      --BbC04y\r\n\
                      Content-Disposition: file; filename=\"file1.txt\"\r\n\
                      \r\n\
                      ... contents of file1.txt ...\r\n\
                      --BbC04y\r\n\
                      Content-Disposition: file; filename=\"awesome_image.gif\"\r\n\
                      Content-Type: image/gif\r\n\
                      Content-Transfer-Encoding: binary\r\n\
                      \r\n\
                      ... contents of awesome_image.gif ...\r\n\
                      --BbC04y--\r\n\
                      --AaB03x--";

        let mut mock = MockStream::with_input(input);

        let mock: &mut NetworkStream = &mut mock;
        let mut stream = BufReader::new(mock);
        let sock: SocketAddr = "127.0.0.1:80".parse().unwrap();
        let req = HyperRequest::new(&mut stream, sock).unwrap();
        let (_, _, headers, _, _, mut reader) = req.deconstruct();

        match read_form_data(&mut reader, &headers) {
            Ok(form_data) => {
                assert_eq!(form_data.fields.len(), 1);
                for (key, val) in form_data.fields {
                    if &key == "submit-name" {
                        assert_eq!(&val, "Larry");
                    }
                }

                assert_eq!(form_data.files.len(), 2);
                for (key, file) in form_data.files {
                    assert_eq!(&key, "files");
                    match &file.filename().unwrap().unwrap()[..] {
                        "file1.txt" => {
                            assert_eq!(file.size, Some(29));
                            assert!(file.content_type().is_none());
                        }
                        "awesome_image.gif" => {
                            assert_eq!(file.size, Some(37));
                            assert_eq!(file.content_type().unwrap(), mime!(Image/Gif));
                        },
                        _ => unreachable!(),
                    }
                }
            },
            Err(err) => panic!("{}", err),
        }
    }

    #[test]
    fn simple_writer() {
        // Create a simple short file for testing
        let tmpdir = tempdir::TempDir::new("form_data_test").unwrap();
        let tmppath = tmpdir.path().join("testfile");
        let mut tmpfile = File::create(tmppath.clone()).unwrap();
        writeln!(tmpfile, "this is example file content").unwrap();

        let mut photo_headers = Headers::new();
        photo_headers.set(ContentType(Mime(TopLevel::Image, SubLevel::Gif, vec![])));
        photo_headers.set(ContentDisposition {
            disposition: DispositionType::Ext("form-data".to_owned()),
            parameters: vec![DispositionParam::Ext("name".to_owned(), "photo".to_owned()),
                             DispositionParam::Ext("filename".to_owned(), "mike.gif".to_owned())],
        });

        let form_data = FormData {
            fields: vec![ ("name".to_owned(), "Mike".to_owned()),
                            ("age".to_owned(), "46".to_owned()) ],
            files: vec![ ("photo".to_owned(), FilePart::new(photo_headers, &tmppath)) ],
        };

        let mut output: Vec<u8> = Vec::new();
        let boundary = generate_boundary();
        match write_form_data(&mut output, &boundary, &form_data) {
            Ok(count) => assert_eq!(count, 568),
            Err(e) => panic!("Unable to write form_data: {}", e),
        }

        println!("{}", String::from_utf8_lossy(&output));
    }


    #[test]
    fn chunked_writer() {
        // Create a simple short file for testing
        let tmpdir = tempdir::TempDir::new("form_data_test").unwrap();
        let tmppath = tmpdir.path().join("testfile");
        let mut tmpfile = File::create(tmppath.clone()).unwrap();
        writeln!(tmpfile, "this is example file content").unwrap();

        let mut photo_headers = Headers::new();
        photo_headers.set(ContentType(Mime(TopLevel::Image, SubLevel::Gif, vec![])));
        photo_headers.set(ContentDisposition {
            disposition: DispositionType::Ext("form-data".to_owned()),
            parameters: vec![DispositionParam::Ext("name".to_owned(), "photo".to_owned()),
                             DispositionParam::Ext("filename".to_owned(), "mike.gif".to_owned())],
        });

        let form_data = FormData {
            fields: vec![ ("name".to_owned(), "Mike".to_owned()),
                            ("age".to_owned(), "46".to_owned()) ],
            files: vec![ ("photo".to_owned(), FilePart::new(photo_headers, &tmppath)) ],
        };

        let mut output: Vec<u8> = Vec::new();
        let boundary = generate_boundary();
        assert!(write_form_data_chunked(&mut output, &boundary, &form_data).is_ok());
        println!("{}", String::from_utf8_lossy(&output));
    }
}
