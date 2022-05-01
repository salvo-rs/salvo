//! Http response.

use std::collections::HashMap;
use std::fmt::{self, Formatter};
use std::str::FromStr;

use cookie::{Cookie, CookieJar};
use http::header::{self, HeaderMap};
use http::method::Method;
pub use http::request::Parts;
use http::version::Version;
use http::{self, Extensions, Uri};
pub use hyper::Body;
use multimap::MultiMap;
use once_cell::sync::OnceCell;
use serde::de::DeserializeOwned;

use crate::addr::SocketAddr;
use crate::http::ParseError;
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
    form_data: tokio::sync::OnceCell<FormData>,
    payload: tokio::sync::OnceCell<Vec<u8>>,

    /// The version of the HTTP protocol used.
    version: Version,
    pub(crate) remote_addr: Option<SocketAddr>,
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
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

impl From<hyper::Request<Body>> for Request {
    fn from(req: hyper::Request<Body>) -> Self {
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
            form_data: tokio::sync::OnceCell::new(),
            payload: tokio::sync::OnceCell::new(),
            // multipart: OnceCell::new(),
            version,
            remote_addr: None,
        }
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
            form_data: tokio::sync::OnceCell::new(),
            payload: tokio::sync::OnceCell::new(),
            version: Version::default(),
            remote_addr: None,
        }
    }
    /// Returns a reference to the associated URI.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// let req = Request::default();
    /// assert_eq!(*req.uri(), *"/");
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
    /// let mut req: Request= Request::default();
    /// *req.uri_mut() = "/hello".parse().unwrap();
    /// assert_eq!(*req.uri(), *"/hello");
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
    /// let req = Request::default();
    /// assert_eq!(*req.method(), Method::GET);
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
    /// Get request remote address.
    #[inline]
    pub fn remote_addr(&self) -> Option<&SocketAddr> {
        self.remote_addr.as_ref()
    }

    /// Returns a reference to the associated header field map.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// let req = Request::default();
    /// assert!(req.headers().is_empty());
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
    /// let mut req: Request = Request::default();
    /// req.headers_mut().insert(HOST, HeaderValue::from_static("world"));
    /// assert!(!req.headers().is_empty());
    /// ```
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        &mut self.headers
    }

    /// Get header with supplied name and try to parse to a 'T', returns None if failed or not found.
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
    /// let req = Request::default();
    /// assert!(req.body().is_some());
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
    /// let req = Request::default();
    /// assert!(req.extensions().get::<i32>().is_none());
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
    /// let mut req = Request::default();
    /// req.extensions_mut().insert("hello");
    /// assert_eq!(req.extensions().get(), Some(&"hello"));
    /// ```
    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }

    /// Get accept.
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

    /// Get first accept.
    #[inline]
    pub fn frist_accept(&self) -> Option<Mime> {
        let mut accept = self.accept();
        if !accept.is_empty() {
            Some(accept.remove(0))
        } else {
            None
        }
    }

    /// Get content type.
    #[inline]
    pub fn content_type(&self) -> Option<Mime> {
        self.headers
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse().ok())
    }

    /// Get `CookieJar` reference.
    #[inline]
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }
    /// Get `CookieJar` mutable reference.
    #[inline]
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }
    /// Get `Cookie` from cookies.
    #[inline]
    pub fn get_cookie<T>(&self, name: T) -> Option<&Cookie<'static>>
    where
        T: AsRef<str>,
    {
        self.cookies.get(name.as_ref())
    }
    /// Get params reference.
    #[inline]
    pub fn params(&self) -> &HashMap<String, String> {
        &self.params
    }
    /// Get params mutable reference.
    #[inline]
    pub fn params_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.params
    }

    /// Get param value from params.
    #[inline]
    pub fn get_param<T>(&self, key: &str) -> Option<T>
    where
        T: FromStr,
    {
        self.params.get(key).and_then(|v| v.parse::<T>().ok())
    }

    /// Get queries reference.
    pub fn queries(&self) -> &MultiMap<String, String> {
        self.queries.get_or_init(|| {
            form_urlencoded::parse(self.uri.query().unwrap_or_default().as_bytes())
                .into_owned()
                .collect()
        })
    }
    /// Get query value from queries.
    #[inline]
    pub fn get_query<F>(&self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.queries().get(key).and_then(|v| v.parse::<F>().ok())
    }

    /// Get field data from form.
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
    /// Get `FilePart` reference from request.
    #[inline]
    pub async fn get_file(&mut self, key: &str) -> Option<&FilePart> {
        self.form_data().await.as_ref().ok().and_then(|ps| ps.files.get(key))
    }
    /// Get `FilePart` lsit reference from request.
    #[inline]
    pub async fn get_files(&mut self, key: &str) -> Option<&Vec<FilePart>> {
        self.form_data()
            .await
            .as_ref()
            .ok()
            .and_then(|ps| ps.files.get_vec(key))
    }
    /// Get value from form first if not found then get from query.
    #[inline]
    pub async fn get_form_or_query<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.get_form(key.as_ref()).await.or_else(|| self.get_query(key))
    }
    /// Get value from query first if not found then get from form.
    #[inline]
    pub async fn get_query_or_form<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.get_query(key.as_ref()).or(self.get_form(key).await)
    }

    /// Get request payload.
    pub async fn payload(&mut self) -> Result<&Vec<u8>, ParseError> {
        let body = self.body.take();
        self.payload
            .get_or_try_init(|| async {
                match body {
                    Some(body) => read_body_bytes(body).await,
                    None => Err(ParseError::EmptyBody),
                }
            })
            .await
    }

    /// Get `FormData` reference from request.
    pub async fn form_data(&mut self) -> Result<&FormData, ParseError> {
        let ctype = self
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if ctype == "application/x-www-form-urlencoded" || ctype.starts_with("multipart/") {
            let body = self.body.take();
            let headers = self.headers();
            self.form_data
                .get_or_try_init(|| async {
                    match body {
                        Some(body) => form::read_form_data(headers, body).await,
                        None => Err(ParseError::EmptyBody),
                    }
                })
                .await
        } else {
            Err(ParseError::NotFormData)
        }
    }

    /// Read body as text from request.
    #[inline]
    pub async fn read_text(&mut self) -> Result<&str, ParseError> {
        self.payload()
            .await
            .and_then(|body| std::str::from_utf8(body).map_err(ParseError::Utf8))
    }

    /// Read body as type `T` from request.
    #[inline]
    pub async fn read_from_text<T>(&mut self) -> Result<T, ParseError>
    where
        T: FromStr,
    {
        self.read_text()
            .await
            .and_then(|body| body.parse::<T>().map_err(|_| ParseError::ParseFromStr))
    }

    /// Read body as type `T` from request.
    #[inline]
    pub async fn read_from_json<T>(&mut self) -> Result<T, ParseError>
    where
        T: DeserializeOwned,
    {
        self.payload()
            .await
            .and_then(|body| serde_json::from_slice::<T>(body).map_err(ParseError::SerdeJson))
    }

    /// Read body as type `T` from request.
    #[inline]
    pub async fn read_from_form<T>(&mut self) -> Result<T, ParseError>
    where
        T: DeserializeOwned,
    {
        self.form_data().await.and_then(|form_data| {
            let data = serde_json::to_value(&form_data.fields)?;
            Ok(serde_json::from_value::<T>(data)?)
        })
    }

    /// Read body as type `T` from request.
    #[inline]
    pub async fn read<T>(&mut self) -> Result<T, ParseError>
    where
        T: DeserializeOwned,
    {
        let ctype = self
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if ctype == "application/x-www-form-urlencoded" || ctype.starts_with("multipart/") {
            self.read_from_form().await
        } else if ctype.starts_with("application/json") {
            self.read_from_json().await
        } else {
            Err(ParseError::InvalidContentType)
        }
    }
}

pub(crate) async fn read_body_bytes(body: Body) -> Result<Vec<u8>, ParseError> {
    hyper::body::to_bytes(body)
        .await
        .map(|d| d.to_vec())
        .map_err(ParseError::Hyper)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::hyper;

    #[tokio::test]
    async fn test_read_text() {
        let mut req: Request = hyper::Request::builder()
            .uri("http://127.0.0.1:7878/hello")
            .header("content-type", "text/plain")
            .body("hello".into())
            .unwrap()
            .into();
        assert_eq!(req.read_from_text::<String>().await.unwrap(), "hello");
    }
    #[tokio::test]
    async fn test_read_json() {
        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct User {
            name: String,
        }
        let mut req: Request = hyper::Request::builder()
            .uri("http://127.0.0.1:7878/hello")
            .header("content-type", "application/json")
            .body(r#"{"name": "jobs"}"#.into())
            .unwrap()
            .into();
        assert_eq!(
            req.read_from_json::<User>().await.unwrap(),
            User { name: "jobs".into() }
        );
    }
    #[tokio::test]
    async fn test_query() {
        let mut req: Request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello?q=rust")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        assert_eq!(req.queries().len(), 1);
        assert_eq!(req.get_query::<String>("q").unwrap(), "rust");
        assert_eq!(req.get_query_or_form::<String>("q").await.unwrap(), "rust");
    }
    #[tokio::test]
    async fn test_form() {
        let mut req: Request = hyper::Request::builder()
            .method("POST")
            .header("content-type", "application/x-www-form-urlencoded")
            .uri("http://127.0.0.1:7979/hello?q=rust")
            .body("lover=dog&money=sh*t&q=firefox".into())
            .unwrap()
            .into();
        assert_eq!(req.get_form::<String>("money").await.unwrap(), "sh*t");
        assert_eq!(req.get_query_or_form::<String>("q").await.unwrap(), "rust");
        assert_eq!(req.get_form_or_query::<String>("q").await.unwrap(), "firefox");

        let mut req: Request = hyper::Request::builder()
            .method("POST")
            .header(
                "content-type",
                "multipart/form-data; boundary=----WebKitFormBoundary0mkL0yrNNupCojyz",
            )
            .uri("http://127.0.0.1:7979/hello?q=rust")
            .body(
                "------WebKitFormBoundary0mkL0yrNNupCojyz\r\n\
Content-Disposition: form-data; name=\"money\"\r\n\r\nsh*t\r\n\
------WebKitFormBoundary0mkL0yrNNupCojyz\r\n\
Content-Disposition: form-data; name=\"file1\"; filename=\"err.txt\"\r\n\
Content-Type: text/plain\r\n\r\n\
file content\r\n\
------WebKitFormBoundary0mkL0yrNNupCojyz--\r\n"
                    .into(),
            )
            .unwrap()
            .into();
        assert_eq!(req.get_form::<String>("money").await.unwrap(), "sh*t");
        let file = req.get_file("file1").await.unwrap();
        assert_eq!(file.file_name().unwrap(), "err.txt");
        assert_eq!(file.headers().get("content-type").unwrap(), "text/plain");
        let files = req.get_files("file1").await.unwrap();
        assert_eq!(files[0].file_name().unwrap(), "err.txt");
    }
}
