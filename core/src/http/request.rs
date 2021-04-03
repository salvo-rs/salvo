use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::net::SocketAddr;
use std::str::FromStr;

use cookie::{Cookie, CookieJar};
use double_checked_cell_async::DoubleCheckedCell;
use http::header::{self, HeaderMap};
use http::method::Method;
pub use http::request::Parts;
use http::version::Version;
use http::{self, Extensions, Uri};
pub use hyper::Body;
use multimap::MultiMap;
use once_cell::sync::OnceCell;
use serde::de::DeserializeOwned;

use crate::http::errors::ReadError;
use crate::http::form::{self, FilePart, FormData};
use crate::http::header::HeaderValue;
use crate::http::Mime;

/// Represents an HTTP request.
///
/// Stores all the properties of the client's request.
pub struct Request {
    // The requested URL.
    uri: Uri,

    // The request headers.
    headers: HeaderMap,

    // The request body as a reader.
    body: Option<Body>,
    extensions: Extensions,

    // The request method.
    method: Method,

    cookies: CookieJar,

    pub(crate) params: HashMap<String, String>,

    // accept: Option<Vec<Mime>>,
    queries: OnceCell<MultiMap<String, String>>,
    form_data: DoubleCheckedCell<FormData>,
    payload: DoubleCheckedCell<Vec<u8>>,

    /// The version of the HTTP protocol used.
    version: Version,
    remote_addr: Option<SocketAddr>,
}

impl Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Request")
            .field("method", self.method())
            .field("uri", self.uri())
            .field("version", &self.version())
            .field("headers", self.headers())
            // omits Extensions because not useful
            .field("body", &self.body())
            .finish()
    }
}

impl Default for Request {
    fn default() -> Request {
        Request::new()
    }
}

impl Request {
    /// Creates a new blank `Request`
    pub fn new() -> Request {
        Request {
            uri: Uri::default(),
            headers: HeaderMap::default(),
            body: Some(Body::default()),
            extensions: Extensions::default(),
            method: Method::default(),
            cookies: CookieJar::default(),
            params: HashMap::new(),
            queries: OnceCell::new(),
            form_data: DoubleCheckedCell::new(),
            payload: DoubleCheckedCell::new(),
            version: Version::default(),
            remote_addr: None,
        }
    }

    /// Create a request from an hyper::Request.
    ///
    /// This constructor consumes the hyper::Request.
    pub fn from_hyper(req: hyper::Request<Body>) -> Request {
        let (
            http::request::Parts {
                method,
                uri,
                version,
                headers,
                extensions,
                ..
            },
            body,
        ) = req.into_parts();

        // Set the request cookies, if they exist.
        let cookies = if let Some(header) = headers.get("Cookie") {
            let mut cookie_jar = CookieJar::new();
            if let Ok(header) = header.to_str() {
                for cookie_str in header.split(';').map(|s| s.trim()) {
                    if let Ok(cookie) = Cookie::parse_encoded(cookie_str).map(|c| c.into_owned()) {
                        cookie_jar.add_original(cookie);
                    }
                }
            }
            cookie_jar
        } else {
            CookieJar::new()
        };

        Request {
            queries: OnceCell::new(),
            uri,
            headers,
            body: Some(body),
            extensions,
            method,
            cookies,
            // accept: None,
            params: HashMap::new(),
            form_data: DoubleCheckedCell::new(),
            payload: DoubleCheckedCell::new(),
            // multipart: OnceCell::new(),
            version,
            remote_addr: None,
        }
    }
    /// Returns a reference to the associated URI.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// let request = Request::default();
    /// assert_eq!(*request.uri(), *"/");
    /// ```
    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Returns a mutable reference to the associated URI.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// let mut request: Request= Request::default();
    /// *request.uri_mut() = "/hello".parse().unwrap();
    /// assert_eq!(*request.uri(), *"/hello");
    /// ```
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.uri
    }

    /// Returns a reference to the associated HTTP method.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// let request = Request::default();
    /// assert_eq!(*request.method(), Method::GET);
    /// ```
    #[inline]
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Returns a mutable reference to the associated HTTP method.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// let mut request: Request = Request::default();
    /// *request.method_mut() = Method::PUT;
    /// assert_eq!(*request.method(), Method::PUT);
    /// ```
    #[inline]
    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.method
    }

    /// Returns the associated version.
    #[inline]
    pub fn version(&self) -> Version {
        self.version
    }
    /// Returns a mutable reference to the associated version.
    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.version
    }
    #[inline]
    pub fn set_remote_addr(&mut self, remote_addr: Option<SocketAddr>) {
        self.remote_addr = remote_addr;
    }
    #[inline]
    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote_addr
    }

    /// Returns a reference to the associated header field map.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// let request = Request::default();
    /// assert!(request.headers().is_empty());
    /// ```
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns a mutable reference to the associated header field map.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// # use salvo_core::http::header::*;
    /// let mut request: Request = Request::default();
    /// request.headers_mut().insert(HOST, HeaderValue::from_static("world"));
    /// assert!(!request.headers().is_empty());
    /// ```
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        &mut self.headers
    }

    /// Get header with supplied name and try to parse to a 'T', return None if failed or not found.
    #[inline]
    pub fn get_header<T>(&self, key: &str) -> Option<T>
    where
        T: FromStr,
    {
        self.headers
            .get(key)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<T>().ok())
    }

    /// Returns a reference to the associated HTTP body.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// let request = Request::default();
    /// assert!(request.body().is_some());
    /// ```
    #[inline]
    pub fn body(&self) -> Option<&Body> {
        self.body.as_ref()
    }
    /// Returns a mutable reference to the associated HTTP body.
    #[inline]
    pub fn body_mut(&mut self) -> Option<&mut Body> {
        self.body.as_mut()
    }

    /// Take body form the request, and set the body to None in the request.
    #[inline]
    pub fn take_body(&mut self) -> Option<Body> {
        self.body.take()
    }

    /// Returns a reference to the associated extensions.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// let request = Request::default();
    /// assert!(request.extensions().get::<i32>().is_none());
    /// ```
    #[inline]
    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    /// Returns a mutable reference to the associated extensions.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// # use salvo_core::http::header::*;
    /// let mut request: Request = Request::default();
    /// request.extensions_mut().insert("hello");
    /// assert_eq!(request.extensions().get(), Some(&"hello"));
    /// ```
    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }

    pub fn accept(&self) -> Vec<Mime> {
        let mut list: Vec<Mime> = vec![];
        if let Some(accept) = self.headers.get("accept").and_then(|h| h.to_str().ok()) {
            let parts: Vec<&str> = accept.split(',').collect();
            for part in parts {
                if let Ok(mt) = part.parse() {
                    list.push(mt);
                }
            }
        }
        list
    }

    #[inline]
    pub fn frist_accept(&self) -> Option<Mime> {
        let mut accept = self.accept();
        if !accept.is_empty() {
            Some(accept.remove(0))
        } else {
            None
        }
    }

    #[inline]
    pub fn content_type(&self) -> Option<Mime> {
        if let Some(ctype) = self.headers.get("content-type").and_then(|h| h.to_str().ok()) {
            ctype.parse().ok()
        } else {
            None
        }
    }

    #[inline]
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }
    #[inline]
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }
    #[inline]
    pub fn get_cookie<T>(&self, name: T) -> Option<&Cookie<'static>>
    where
        T: AsRef<str>,
    {
        self.cookies.get(name.as_ref())
    }
    #[inline]
    pub fn params(&self) -> &HashMap<String, String> {
        &self.params
    }
    #[inline]
    pub fn params_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.params
    }

    #[inline]
    pub fn get_param<T>(&self, key: &str) -> Option<T>
    where
        T: FromStr,
    {
        self.params.get(key).and_then(|v| v.parse::<T>().ok())
    }

    pub fn queries(&self) -> &MultiMap<String, String> {
        self.queries.get_or_init(|| {
            form_urlencoded::parse(self.uri.query().unwrap_or_default().as_bytes())
                .into_owned()
                .collect()
        })
    }
    #[inline]
    pub fn get_query<F>(&self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.queries().get(key).and_then(|v| v.parse::<F>().ok())
    }

    #[inline]
    pub async fn get_form<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.form_data()
            .await
            .as_ref()
            .ok()
            .and_then(|ps| ps.fields.get(key))
            .and_then(|v| v.parse::<F>().ok())
    }
    #[inline]
    pub async fn get_file(&mut self, key: &str) -> Option<&FilePart> {
        self.form_data().await.as_ref().ok().and_then(|ps| ps.files.get(key))
    }
    #[inline]
    pub async fn get_files(&mut self, key: &str) -> Option<&Vec<FilePart>> {
        self.form_data()
            .await
            .as_ref()
            .ok()
            .and_then(|ps| ps.files.get_vec(key))
    }
    #[inline]
    pub async fn get_form_or_query<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.get_form(key.as_ref()).await.or_else(|| self.get_query(key))
    }
    #[inline]
    pub async fn get_query_or_form<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.get_query(key.as_ref()).or(self.get_form(key).await)
    }
    pub async fn payload(&mut self) -> Result<&Vec<u8>, ReadError> {
        let ctype = self
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if ctype == "application/x-www-form-urlencoded" || ctype.starts_with("multipart/form-data") {
            Err(ReadError::General(String::from("failed to read data1")))
        } else if ctype.starts_with("application/json") || ctype.starts_with("text/") {
            let body = self.body.take();
            self.payload
                .get_or_try_init(async {
                    match body {
                        Some(body) => read_body_bytes(body).await,
                        None => Err(ReadError::General(String::from("failed to read data2"))),
                    }
                })
                .await
        } else {
            Err(ReadError::General(String::from("failed to read data3")))
        }
    }

    pub async fn form_data(&mut self) -> Result<&FormData, ReadError> {
        let ctype = self
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if ctype == "application/x-www-form-urlencoded" || ctype.starts_with("multipart/form-data") {
            let body = self.body.take();
            let headers = self.headers();
            self.form_data
                .get_or_try_init(async {
                    match body {
                        Some(body) => form::read_form_data(headers, body).await,
                        None => Err(ReadError::General("empty body".into())),
                    }
                })
                .await
        } else {
            Err(ReadError::General("failed to read form data".into()))
        }
    }

    #[inline]
    pub async fn read_text(&mut self) -> Result<&str, ReadError> {
        match self.payload().await {
            Ok(body) => Ok(std::str::from_utf8(&body)?),
            Err(_) => Err(ReadError::General("read text from body failed".into())),
        }
    }
    #[inline]
    pub async fn read_from_text<T>(&mut self) -> Result<T, ReadError>
    where
        T: FromStr,
    {
        self.read_text()
            .await
            .and_then(|body| body.parse::<T>().map_err(|_| ReadError::Parsing(body.into())))
    }
    #[inline]
    pub async fn read_from_json<T>(&mut self) -> Result<T, ReadError>
    where
        T: DeserializeOwned,
    {
        match self.payload().await {
            Ok(body) => Ok(serde_json::from_slice::<T>(body)?),
            Err(_) => Err(ReadError::General("read json from body failed".into())),
        }
    }
    #[inline]
    pub async fn read_from_form<T>(&mut self) -> Result<T, ReadError>
    where
        T: DeserializeOwned,
    {
        match self.form_data().await {
            Ok(form_data) => {
                let data = serde_json::to_value(&form_data.fields)?;
                Ok(serde_json::from_value::<T>(data)?)
            }
            Err(_) => Err(ReadError::General("read data from form failed".into())),
        }
    }

    #[inline]
    pub async fn read<T>(&mut self) -> Result<T, ReadError>
    where
        T: DeserializeOwned,
    {
        let ctype = self
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if ctype == "application/x-www-form-urlencoded" || ctype.starts_with("multipart/form-data") {
            self.read_from_form().await
        } else if ctype.starts_with("application/json") {
            self.read_from_json().await
        } else {
            Err(ReadError::General(String::from(
                "failed to read data or this type is not supported",
            )))
        }
    }
}

pub trait BodyReader: Send {}
// pub(crate) async fn read_body_cursor<B: HttpBody>(body: B) -> Result<Cursor<Vec<u8>>, ReadError> {
//     Ok(Cursor::new(read_body_bytes(body).await?))
// }
pub(crate) async fn read_body_bytes(body: Body) -> Result<Vec<u8>, ReadError> {
    hyper::body::to_bytes(body)
        .await
        .map_err(|_| ReadError::General("read body bytes error".into()))
        .map(|d| d.to_vec())
}
