use std::fmt::{self, Debug};
use std::fs::File;
use std::io::{self, Write};

// use typemap::TypeMap;

use http::StatusCode;
use cookie::Cookie;
use crate::http::headers::{self, HeaderMap};

use hyper::Body;
use hyper::Method;

// pub trait Content: Send {
//     fn media_type(&self)->&Mime;
//     fn body_writer(&self)->&BodyWriter;
//     fn take(self)->(Mime, BodyWriter);
// }
// pub struct HtmlContent{
//     media_type: Mime,
//     body_writer: String,
// }
// impl HtmlContent {
//     pub fn new<T:Into<String>>(body_writer: T)->HtmlContent{
//         HtmlContent{
//             media_type: Mime::new("text", "html"),
//             body_writer: body_writer.into(),
//         }
//     }
// }
// impl Content for HtmlContent {
//     fn media_type(&self)->&Mime{
//         &self.media_type
//     }
//     fn body_writer(&self)->&impl BodyWriter{
//         &self.body_writer
//     }
//     fn take(self)->(Mime, impl BodyWriter){
//         (self.media_type, self.body_writer)
//     }
// }

/// A trait which writes the body of an HTTP response.
pub trait BodyWriter: Send {
    /// Writes the body to the provided `Write`.
    fn write_body(&mut self, res: &mut dyn Write) -> io::Result<()>;
}

impl BodyWriter for String {
    fn write_body(&mut self, res: &mut dyn Write) -> io::Result<()> {
        self.as_bytes().write_body(res)
    }
}

impl<'a> BodyWriter for &'a str {
    fn write_body(&mut self, res: &mut dyn Write) -> io::Result<()> {
        self.as_bytes().write_body(res)
    }
}

impl BodyWriter for Vec<u8> {
    fn write_body(&mut self, res: &mut dyn Write) -> io::Result<()> {
        res.write_all(self)
    }
}

impl<'a> BodyWriter for &'a [u8] {
    fn write_body(&mut self, res: &mut dyn Write) -> io::Result<()> {
        res.write_all(self)
    }
}

impl BodyWriter for File {
    fn write_body(&mut self, res: &mut dyn Write) -> io::Result<()> {
        io::copy(self, res).map(|_| ())
    }
}

impl BodyWriter for Box<dyn io::Read + Send> {
    fn write_body(&mut self, res: &mut dyn Write) -> io::Result<()> {
        io::copy(self, res).map(|_| ())
    }
}

/// The response representation given to `Middleware`
pub struct Response {
    /// The response status-code.
    pub status: Option<StatusCode>,

    /// The headers of the response.
    pub headers: HeaderMap,

    /// The body_writers of the response.
    pub body_writers: Vec<Box<dyn BodyWriter>>,
}

impl Default for Response {
    fn default() -> Self {
        Self::new()
    }
}

impl Response {
    /// Construct a blank Response
    pub fn new() -> Response {
        Response {
            status: None, // Start with no response code.
            body_writers: Vec::new(),   // Start with no body_writers.
            headers: HeaderMap::new(),
        }
    }

    #[inline(always)]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }
    
    // `write_back` is used to put all the data added to `self`
    // back onto an `hyper::Response` so that it is sent back to the
    // client.
    //
    // `write_back` consumes the `Response`.
    #[doc(hidden)]
    pub fn write_back(self, http_res: &mut hyper::Response<Body>, req_method: Method) {
        *http_res.headers_mut() = self.headers;

        // Default to a 404 if no response code was set
        *http_res.status_mut() = self.status.unwrap_or(StatusCode::NOT_FOUND);

        if let Method::HEAD = req_method {
            return 
        }else{
            if self.body_writers.len() == 0 {
                http_res.headers_mut().insert(
                    headers::CONTENT_LENGTH,
                    headers::HeaderValue::from_static("0"),
                );
            }else{
                for writer in self.body_writers {
                    write_with_body(http_res, writer).ok();
                } 
            }
        }
    }

    pub fn cookies(&self) -> Vec<Cookie<'_>> {
        let mut cookies = vec![];
        for header in self.headers().get(headers::SET_COOKIE) {
            if let Ok(header) = header.to_str() {
                if let Ok(cookie) = Cookie::parse_encoded(header) {
                    cookies.push(cookie);
                }
            }
        }

        cookies
    }

    // #[inline(always)]
    // pub fn content_type(&self) -> Option<Mime> {
    //     self.headers.get_one("Content-Type").and_then(|v| v.parse().ok())
    // }
}

fn write_with_body(res: &mut hyper::Response<Body>, mut body: Box<dyn BodyWriter>) -> io::Result<()> {
    let content_type = res.headers().get(headers::CONTENT_TYPE).map_or_else(
        || headers::HeaderValue::from_static("text/plain"),
        |cx| cx.clone(),
    );
    res.headers_mut().insert(headers::CONTENT_TYPE, content_type);

    let mut body_contents: Vec<u8> = vec![];
    body.write_body(&mut body_contents)?;
    *res.body_mut() = Body::from(body_contents);
    Ok(())
}

impl Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "HTTP/1.1 {}\n{:?}",
            self.status.unwrap_or(StatusCode::NOT_FOUND),
            self.headers
        )
    }
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(self, f)
    }
}