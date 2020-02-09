use std::fmt::{self, Debug};
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::borrow::Cow;
use std::sync::Arc;

use http::StatusCode;
use cookie::{Cookie, CookieJar};
use hyper::Body;
use hyper::Method;
use mime::Mime;
use serde::{Deserialize, Serialize};

use crate::{Content, ServerConfig};
use crate::http::errors::HttpError;
use crate::http::header::SET_COOKIE;
use crate::logging;
use crate::http::header::{self, HeaderMap};


/// A trait which writes the body of an HTTP response.
pub trait BodyWriter: Send {
    /// Writes the body to the provided `Write`.
    fn write_body(&mut self, res: &mut dyn Write) -> std::io::Result<()>;
}

impl BodyWriter for String {
    fn write_body(&mut self, res: &mut dyn Write) -> std::io::Result<()> {
        self.as_bytes().write_body(res)
    }
}

impl<'a> BodyWriter for &'a str {
    fn write_body(&mut self, res: &mut dyn Write) -> std::io::Result<()> {
        self.as_bytes().write_body(res)
    }
}

impl BodyWriter for Vec<u8> {
    fn write_body(&mut self, res: &mut dyn Write) -> std::io::Result<()> {
        res.write_all(self)
    }
}

impl<'a> BodyWriter for &'a [u8] {
    fn write_body(&mut self, res: &mut dyn Write) -> std::io::Result<()> {
        res.write_all(self)
    }
}

impl BodyWriter for File {
    fn write_body(&mut self, res: &mut dyn Write) -> std::io::Result<()> {
        std::io::copy(self, res).map(|_| ())
    }
}

impl BodyWriter for Box<dyn std::io::Read + Send> {
    fn write_body(&mut self, res: &mut dyn Write) -> std::io::Result<()> {
        std::io::copy(self, res).map(|_| ())
    }
}

/// The response representation given to `Middleware`
pub struct Response {
    /// The response status-code.
    status_code: Option<StatusCode>,

    /// The headers of the response.
    headers: HeaderMap,

    pub(crate) cookies: CookieJar,

    /// The body_writers of the response.
    pub(crate) body_writers: Vec<Box<dyn BodyWriter>>,
    pub(crate) server_config: Arc<ServerConfig>,

    is_commited: bool,
}

// impl Default for Response {
//     fn default() -> Self {
//         Self::new()
//     }
// }

impl Response {
    /// Construct a blank Response
    pub fn new(sconf: Arc<ServerConfig>) -> Response {
        Response {
            status_code: None, // Start with no response code.
            body_writers: Vec::new(),   // Start with no body_writers.
            headers: HeaderMap::new(),
            cookies: CookieJar::new(),
            server_config: sconf,
            is_commited: false,
        }
    }

    #[inline(always)]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }
    // pub fn insert_header<K>(&mut self, key: K, val: T) -> Option<T> where K: IntoHeaderName,
    //     self.headers.insert(key, val)
    // }
    
    // `write_back` is used to put all the data added to `self`
    // back onto an `hyper::Response` so that it is sent back to the
    // client.
    //
    // `write_back` consumes the `Response`.
    #[doc(hidden)]
    pub fn write_back(self, http_res: &mut hyper::Response<Body>, req_method: Method) {
        *http_res.headers_mut() = self.headers;

        // Default to a 404 if no response code was set
        *http_res.status_mut() = self.status_code.unwrap_or(StatusCode::NOT_FOUND);

        if let Method::HEAD = req_method {
            return 
        }else{
            if self.body_writers.is_empty() {
                http_res.headers_mut().insert(
                    header::CONTENT_LENGTH,
                    header::HeaderValue::from_static("0"),
                );
            }else{
                for writer in self.body_writers {
                    write_with_body(http_res, writer).ok();
                } 
            }
        }
    }

    #[inline(always)]
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }
    pub fn header_cookies(&self) -> Vec<Cookie<'_>> {
        let mut cookies = vec![];
        for header in self.headers().get_all(header::SET_COOKIE).iter() {
            if let Ok(header) = header.to_str() {
                if let Ok(cookie) = Cookie::parse_encoded(header) {
                    cookies.push(cookie);
                }
            }
        }
        cookies
    }

    #[inline]
    pub fn get_cookie<T>(&self, name:T) -> Option<&Cookie<'static>> where T: AsRef<str> {
         self.cookies.get(name.as_ref())
    }
    #[inline]
    pub fn add_cookie(&mut self, cookie: Cookie<'static>) {
        self.cookies.add(cookie);
    }
    #[inline]
    pub fn remove_cookie<T>(&mut self, name: T) where T: Into<Cow<'static, str>> {
        self.cookies.remove(Cookie::named(name));
    }
    #[inline]
    pub fn status_code(&mut self) -> Option<StatusCode> {
        self.status_code
    }
    
    #[inline]
    pub fn set_status_code(&mut self, code: StatusCode) {
        self.status_code = Some(code);
    }

    // #[inline(always)]
    // pub fn content_type(&self) -> Option<Mime> {
    //     self.headers.get_one("Content-Type").and_then(|v| v.parse().ok())
    // }
    
    #[inline]
    pub fn write_error(&mut self, err: impl HttpError){
        self.status_code = Some(err.code());
        self.commit();
    }
    #[inline]
    pub fn write_content(&mut self, content: impl Content){
        content.apply(self);
    }
    #[inline]
    pub fn write_body(&mut self, writer: impl BodyWriter+'static) {
        self.body_writers.push(Box::new(writer))
    }
    #[inline]
    pub fn render_cbor<'a, T: Serialize>(&mut self, writer: &'a T) {
        if let Ok(data) = serde_cbor::to_vec(writer) {
            self.render("application/cbor", data);
        } else {
            self.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
            let emsg = ErrorWrap::new("server_error", "server error", "error when serialize object to cbor");
            self.render("application/cbor", serde_cbor::to_vec(&emsg).unwrap());
        }
    }
    #[inline]
    pub fn render_json<'a, T: Serialize>(&mut self, writer: &'a T) {
        if let Ok(data) = serde_json::to_string(writer) {
            self.render("application/json", data);
        } else {
            self.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
            let emsg = ErrorWrap::new("server_error", "server error", "error when serialize object to json");
            self.render("application/json", serde_json::to_string(&emsg).unwrap());
        }
    }
    pub fn render_json_text<T: Into<String>>(&mut self, writer: T) {
        self.render("application/json", writer.into());
    }
    #[inline]
    pub fn render_html_text<T: Into<String>>(&mut self, writer: T) {
        self.render("text/html", writer.into());
    }
    #[inline]
    pub fn render_plain_text<T: Into<String>>(&mut self, writer: T) {
        self.render("text/plain", writer.into());
    }
    #[inline]
    pub fn render_xml_text<T: Into<String>>(&mut self, writer: T) {
        self.render("text/xml", writer.into());
    }
    // RenderBinary is like RenderFile() except that it instead of a file on disk,
    // it renders store from memory (which could be a file that has not been written,
    // the output from some function, or bytes streamed from somewhere else, as long
    // it implements io.Reader).  When called directly on something generated or
    // streamed, modtime should mostly likely be time.Now().
    #[inline]
    pub fn render_binary<T>(&mut self, content_type:T, data: Vec<u8>) where T: AsRef<str> {
        self.render(content_type, data);
    }
    #[inline]
    pub fn render_file<T>(&mut self, content_type:T, file: &mut File)  where T: AsRef<str> {
        let mut data = Vec::new();  
        if file.read_to_end(&mut data).is_err() {
            return self.not_found();
        }
        self.render_binary(content_type, data);
    }
    #[inline]
    pub fn render_file_from_path<T>(&mut self, path: T) where T: AsRef<Path> {
        match File::open(path.as_ref()) {
            Ok(mut file) => {
                if let Some(mime) = self.get_mime_by_path(path.as_ref().to_str().unwrap_or("")) {
                    self.render_file(mime.to_string(), &mut file);
                }else{
                    self.unsupported_media_type();
                    error!(logging::logger(), "error on render file from path"; "path" => path.as_ref().to_str());
                }
            },
            Err(_) => {
                self.not_found();
            },
        }
    }
    #[inline]
    pub fn render<T>(&mut self, content_type:T, writer: impl BodyWriter+'static) where T: AsRef<str> {
        self.headers.insert(header::CONTENT_TYPE, content_type.as_ref().parse().unwrap());
        self.write_body(writer);
    }
    
    #[inline]
    pub fn send_binary<T>(&mut self, data: Vec<u8>, file_name: T) where T: AsRef<str> {
        let file_name = Path::new(file_name.as_ref()).file_name().and_then(|s|s.to_str()).unwrap_or("file.dat");
        if let Some(mime) = self.get_mime_by_path(file_name) {
            self.headers.insert(header::CONTENT_DISPOSITION, format!("attachment; filename={}", &file_name).parse().unwrap());
            self.render(mime.to_string(), data);
        }else{
            self.unsupported_media_type();
            error!(logging::logger(), "error on send binary"; "file_name" => AsRef::<str>::as_ref(&file_name));
        }
    }
    #[inline]
    pub fn send_file<T>(&mut self, file: &mut File, file_name: T) -> std::io::Result<()> where T: AsRef<str> {
        let mut data = Vec::new();  
        file.read_to_end(&mut data)?;
        self.send_binary(data, file_name.as_ref());
        Ok(())
    }
    #[inline]
    pub fn send_file_from_path<T>(&mut self, path: T, file_name: Option<T>) -> std::io::Result<()> where T: AsRef<str> {
        let mut file = File::open(path.as_ref())?;
        self.send_file(&mut file, file_name.unwrap_or(path))
    }
    
    #[inline]
    pub fn redirect_temporary<U: AsRef<str>>(&mut self, url: U) {
        self.status_code = Some(StatusCode::MOVED_PERMANENTLY);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers.insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse().unwrap());
    }
    #[inline]
    pub fn redirect_found<U: AsRef<str>>(&mut self, url: U) {
        self.status_code = Some(StatusCode::FOUND);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers.insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse().unwrap());
    }
    #[inline]
    pub fn redirect_other<U: AsRef<str>>(&mut self, url: U) {
        self.status_code = Some(StatusCode::SEE_OTHER);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers.insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse().unwrap());
    }
    #[inline]
    pub fn commit(&mut self) {
        for cookie in self.cookies.delta() {
            if let Ok(hv) = cookie.encoded().to_string().parse(){
                self.headers.append(SET_COOKIE, hv);
            }
        }
        self.is_commited = true;
    }
    #[inline]
    pub fn is_commited(&self) -> bool{
        self.is_commited
    }

    #[inline]
    pub fn not_found(&mut self) {
        self.status_code = Some(StatusCode::NOT_FOUND);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers.insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
        }
        self.commit();
    }

    #[inline]
    pub fn unauthorized(&mut self) {
        self.status_code = Some(StatusCode::UNAUTHORIZED);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers.insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
        }
        self.commit();
    }

    #[inline]
    pub fn forbidden(&mut self) {
        self.status_code = Some(StatusCode::FORBIDDEN);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers.insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
        }
        self.commit();
    }
    #[inline]
    pub fn unsupported_media_type(&mut self) {
        self.status_code = Some(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        self.commit();
    }
    
    fn get_mime_by_path<T>(&self, path:T) -> Option<Mime> where T:AsRef<str> {
        let guess = mime_guess::from_path(path.as_ref());
        if let Some(mime) = guess.first() {
            for m in &*self.server_config.allowed_media_types {
                if m.type_() == mime.type_() && m.subtype() == mime.subtype() {
                    return Some(mime);
                }
            }
        }
        None
    }
}

fn write_with_body(resp: &mut hyper::Response<Body>, mut body: Box<dyn BodyWriter>) -> std::io::Result<()> {
    let content_type = resp.headers().get(header::CONTENT_TYPE).map_or_else(
        || header::HeaderValue::from_static("text/html"),
        |cx| cx.clone(),
    );
    resp.headers_mut().insert(header::CONTENT_TYPE, content_type);

    let mut body_contents: Vec<u8> = vec![];
    body.write_body(&mut body_contents)?;
    *resp.body_mut() = Body::from(body_contents);
    Ok(())
}

impl Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "HTTP/1.1 {}\n{:?}",
            self.status_code.unwrap_or(StatusCode::NOT_FOUND),
            self.headers
        )
    }
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(self, f)
    }
}


#[derive(Serialize, Deserialize, Debug)]
struct ErrorInfo {
    name: String,
    summary: String,
    detail: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct ErrorWrap {
    error: ErrorInfo,
}

impl ErrorWrap{
    pub fn new<N, S, D>(name:N, summary: S, detail: D) -> ErrorWrap where N: Into<String>, S: Into<String>, D: Into<String> {
        ErrorWrap {
            error: ErrorInfo {
                name: name.into(),
                summary: summary.into(),
                detail: detail.into(),
            },
        }
    }
}
