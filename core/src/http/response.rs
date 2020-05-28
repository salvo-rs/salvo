use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt::{self, Debug};
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use bytes::{Bytes, BytesMut};
use cookie::{Cookie, CookieJar};
use futures::Stream;
use futures::TryStreamExt;
use http::StatusCode;
use httpdate::HttpDate;
use hyper::header::*;
use hyper::Method;
use mime::Mime;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use super::errors::HttpError;
use super::header::SET_COOKIE;
use super::header::{self, HeaderMap, CONTENT_DISPOSITION};
use crate::http::Request;
use crate::logging;
use crate::logging::logger;
use crate::ServerConfig;

#[allow(clippy::type_complexity)]
pub enum ResponseBody {
    None,
    Empty,
    Bytes(BytesMut),
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes, Box<dyn StdError + Send + Sync>>> + Send + Sync>>),
}
/// The response representation given to `Middleware`
pub struct Response {
    /// The response status-code.
    status_code: Option<StatusCode>,
    pub(crate) http_error: Option<HttpError>,

    /// The headers of the response.
    headers: HeaderMap,

    pub(crate) cookies: CookieJar,

    pub(crate) body: ResponseBody,
    pub(crate) server_config: Arc<ServerConfig>,

    is_commited: bool,
}

impl Response {
    /// Construct a blank Response
    pub fn new(conf: Arc<ServerConfig>) -> Response {
        Response {
            status_code: None, // Start with no response code.
            http_error: None,
            body: ResponseBody::None, // Start with no writers.
            headers: HeaderMap::new(),
            cookies: CookieJar::new(),
            server_config: conf,
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
    pub async fn write_back(self, req: &mut Request, res: &mut hyper::Response<hyper::Body>) {
        *res.headers_mut() = self.headers;

        // Default to a 404 if no response code was set
        *res.status_mut() = self.status_code.unwrap_or(StatusCode::NOT_FOUND);

        if let Method::HEAD = *req.method() {
        } else {
            match self.body {
                ResponseBody::Bytes(bytes) => {
                    *res.body_mut() = hyper::Body::from(Bytes::from(bytes));
                }
                ResponseBody::Stream(stream) => {
                    *res.body_mut() = hyper::Body::wrap_stream(stream);
                }
                _ => {
                    println!(">>>>>>>>>>>????????/");
                    res.headers_mut().insert(header::CONTENT_LENGTH, header::HeaderValue::from_static("0"));
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
    pub fn get_cookie<T>(&self, name: T) -> Option<&Cookie<'static>>
    where
        T: AsRef<str>,
    {
        self.cookies.get(name.as_ref())
    }
    #[inline]
    pub fn add_cookie(&mut self, cookie: Cookie<'static>) {
        self.cookies.add(cookie);
    }
    #[inline]
    pub fn remove_cookie<T>(&mut self, name: T)
    where
        T: Into<Cow<'static, str>>,
    {
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
    pub fn set_http_error(&mut self, err: HttpError) {
        self.status_code = Some(err.code);
        self.http_error = Some(err);
        self.commit();
    }
    // #[inline]
    // pub fn render_cbor<'a, T: Serialize>(&mut self, writer: &'a T) {
    //     if let Ok(data) = serde_cbor::to_vec(writer) {
    //         self.render("application/cbor", data);
    //     } else {
    //         self.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
    //         let emsg = ErrorWrap::new("server_error", "server error", "error when serialize object to cbor");
    //         self.render("application/cbor", serde_cbor::to_vec(&emsg).unwrap());
    //     }
    // }
    #[inline]
    pub fn render_json<'a, T: Serialize>(&mut self, data: &'a T) {
        if let Ok(data) = serde_json::to_string(data) {
            self.render("application/json", data.as_bytes());
        } else {
            self.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
            let emsg = ErrorWrap::new("server_error", "server error", "error when serialize object to json");
            self.render("application/json", serde_json::to_string(&emsg).unwrap().as_bytes());
        }
    }
    pub fn render_json_text(&mut self, data: &str) {
        self.render("application/json", data.as_bytes());
    }
    #[inline]
    pub fn render_html_text(&mut self, data: &str) {
        self.render("text/html", data.as_bytes());
    }
    #[inline]
    pub fn render_plain_text(&mut self, data: &str) {
        self.render("text/plain", data.as_bytes());
    }
    #[inline]
    pub fn render_xml_text(&mut self, data: &str) {
        self.render("text/xml", data.as_bytes());
    }
    // RenderBinary is like RenderFile() except that it instead of a file on disk,
    // it renders store from memory (which could be a file that has not been written,
    // the output from some function, or bytes streamed from somewhere else, as long
    // it implements io.Reader).  When called directly on something generated or
    // streamed, modtime should mostly likely be time.Now().
    #[inline]
    pub fn render_binary(&mut self, content_type: &str, data: &[u8]) {
        self.render(content_type, data);
    }
    // #[inline]
    // pub fn render_file<T>(&mut self, content_type: &str, file: &mut File)  where T: AsRef<str> {
    //     let mut data = Vec::new();
    //     if file.read_to_end(&mut data).is_err() {
    //         return self.not_found();
    //     }
    //     self.render_binary(content_type, data);
    // }
    // #[inline]
    // pub fn render_file_with_name<T>(&mut self, content_type: &str, file: &mut File, name: &str)  where T: AsRef<str> {
    //     self.headers_mut().append(CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", name).parse().unwrap());
    //     self.render_file(content_type, file);
    // }
    // #[inline]
    // pub fn render_file_from_path<T>(&mut self, path: T) where T: AsRef<Path> {
    //     match File::open(path.as_ref()) {
    //         Ok(mut file) => {
    //             if let Some(mime) = self.get_mime_by_path(path.as_ref().to_str().unwrap_or("")) {
    //                 self.render_file(mime.to_string(), &mut file);
    //             }else{
    //                 self.unsupported_media_type();
    //                 error!(logging::logger(), "error on render file from path"; "path" => path.as_ref().to_str());
    //             }
    //         },
    //         Err(_) => {
    //             self.not_found();
    //         },
    //     }
    // }

    // #[inline]
    // pub fn render_file_from_path_with_name<T>(&mut self, path: T, name: &str) where T: AsRef<Path> {
    //     match File::open(path.as_ref()) {
    //         Ok(mut file) => {
    //             if let Some(mime) = self.get_mime_by_path(path.as_ref().to_str().unwrap_or("")) {
    //                 self.render_file_with_name(mime.to_string(), &mut file, name);
    //             }else{
    //                 self.unsupported_media_type();
    //                 error!(logging::logger(), "error on render file from path"; "path" => path.as_ref().to_str());
    //             }
    //         },
    //         Err(_) => {
    //             self.not_found();
    //         },
    //     }
    // }
    #[inline]
    pub fn render(&mut self, content_type: &str, data: &[u8]) {
        self.headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
        self.write_body_bytes(data);
    }

    #[inline]
    pub fn write_body_bytes(&mut self, data: &[u8]) {
        match &mut self.body {
            ResponseBody::Bytes(bytes) => {
                bytes.extend_from_slice(data);
            }
            ResponseBody::Stream(_) => {
                warn!(logger(), "Current body kind is stream, try to write bytes to it");
                self.body = ResponseBody::Bytes(BytesMut::from(data));
            }
            _ => {
                self.body = ResponseBody::Bytes(BytesMut::from(data));
            }
        }
    }
    #[inline]
    pub fn streaming<S, O, E>(&mut self, stream: S)
    where
        S: Stream<Item = Result<O, E>> + Send + Sync + 'static,
        O: Into<Bytes> + 'static,
        E: Into<Box<dyn StdError + Send + Sync>> + 'static,
    {
        match self.body {
            ResponseBody::Bytes(_) => {
                warn!(logger(), "Current body kind is bytes already");
            }
            ResponseBody::Stream(_) => {
                warn!(logger(), "Current body kind is stream already");
            }
            _ => {}
        }
        let mapped = stream.map_ok(Into::into).map_err(Into::into);
        self.body = ResponseBody::Stream(Box::pin(mapped));
    }

    #[inline]
    pub fn send_binary(&mut self, data: &[u8], file_name: &str) {
        let file_name = Path::new(file_name).file_name().and_then(|s| s.to_str()).unwrap_or("file.dat");
        if let Some(mime) = self.get_mime_by_path(file_name) {
            self.headers.insert(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", &file_name).parse().unwrap(),
            );
            self.render(&mime.to_string(), data);
        } else {
            self.unsupported_media_type();
            error!(logging::logger(), "error on send binary"; "file_name" => AsRef::<str>::as_ref(&file_name));
        }
    }
    // #[inline]
    // pub fn send_file<T>(&mut self, file: &mut File, file_name: T) -> std::io::Result<()> where T: AsRef<str> {
    //     let mut data = Vec::new();
    //     file.read_to_end(&mut data)?;
    //     self.send_binary(data, file_name.as_ref());
    //     Ok(())
    // }
    // #[inline]
    // pub fn send_file_from_path<T>(&mut self, path: T, file_name: Option<T>) -> std::io::Result<()> where T: AsRef<str> {
    //     let mut file = File::open(path.as_ref())?;
    //     self.send_file(&mut file, file_name.unwrap_or(path))
    // }

    #[inline]
    pub fn redirect_temporary<U: AsRef<str>>(&mut self, url: U) {
        self.status_code = Some(StatusCode::MOVED_PERMANENTLY);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers.insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse().unwrap());
        self.commit();
    }
    #[inline]
    pub fn redirect_found<U: AsRef<str>>(&mut self, url: U) {
        self.status_code = Some(StatusCode::FOUND);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers.insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse().unwrap());
        self.commit();
    }
    #[inline]
    pub fn redirect_other<U: AsRef<str>>(&mut self, url: U) {
        self.status_code = Some(StatusCode::SEE_OTHER);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers.insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse().unwrap());
        self.commit();
    }
    pub fn set_content_disposition(&mut self, value: &str) {
        self.headers_mut().insert(CONTENT_DISPOSITION, value.parse().unwrap());
    }
    pub fn set_content_encoding(&mut self, value: &str) {
        self.headers_mut().insert(CONTENT_ENCODING, value.parse().unwrap());
    }
    pub fn set_content_length(&mut self, value: u64) {
        self.headers_mut().insert(CONTENT_LENGTH, value.to_string().parse().unwrap());
    }
    pub fn set_content_range(&mut self, value: &str) {
        self.headers_mut().insert(CONTENT_RANGE, value.parse().unwrap());
    }
    pub fn set_content_type(&mut self, value: &str) {
        self.headers_mut().insert(CONTENT_TYPE, value.parse().unwrap());
    }
    pub fn set_accept_range(&mut self, value: &str) {
        self.headers_mut().insert(ACCEPT_RANGES, value.parse().unwrap());
    }
    pub fn set_last_modified(&mut self, value: HttpDate) {
        self.headers_mut().insert(LAST_MODIFIED, format!("{}", value).parse().unwrap());
    }
    pub fn set_etag(&mut self, value: &str) {
        self.headers_mut().insert(ETAG, value.parse().unwrap());
    }
    #[inline]
    pub fn commit(&mut self) {
        for cookie in self.cookies.delta() {
            if let Ok(hv) = cookie.encoded().to_string().parse() {
                self.headers.append(SET_COOKIE, hv);
            }
        }
        self.is_commited = true;
    }
    #[inline]
    pub fn is_commited(&self) -> bool {
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

    fn get_mime_by_path<T>(&self, path: T) -> Option<Mime>
    where
        T: AsRef<str>,
    {
        let guess = mime_guess::from_path(path.as_ref());
        if let Some(mime) = guess.first() {
            if self.server_config.allowed_media_types.len() > 0 {
                for m in &*self.server_config.allowed_media_types {
                    if m.type_() == mime.type_() && m.subtype() == mime.subtype() {
                        return Some(mime);
                    }
                }
            } else {
                return Some(mime);
            }
        }
        None
    }
}

impl Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "HTTP/1.1 {}\n{:?}", self.status_code.unwrap_or(StatusCode::NOT_FOUND), self.headers)
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

impl ErrorWrap {
    pub fn new<N, S, D>(name: N, summary: S, detail: D) -> ErrorWrap
    where
        N: Into<String>,
        S: Into<String>,
        D: Into<String>,
    {
        ErrorWrap {
            error: ErrorInfo {
                name: name.into(),
                summary: summary.into(),
                detail: detail.into(),
            },
        }
    }
}
