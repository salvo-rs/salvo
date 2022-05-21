//! Http response.

use std::collections::HashMap;
use std::fmt::{self, Formatter};
use std::str::FromStr;

use cookie::{Cookie, CookieJar};
use enumflags2::{bitflags, BitFlags};
use http::header::{self, HeaderMap};
use http::method::Method;
pub use http::request::Parts;
use http::version::Version;
use http::{self, Extensions, Uri};
pub use hyper::Body;
use multimap::MultiMap;
use once_cell::sync::OnceCell;
use serde::de::{Deserialize, DeserializeOwned};

use crate::addr::SocketAddr;
use crate::de::{from_str_map, from_str_multi_map};
use crate::http::form::{FilePart, FormData};
use crate::http::header::HeaderValue;
use crate::http::Mime;
use crate::http::ParseError;

/// ParseSource
#[bitflags]
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ParseSource {
    /// Parse from url router params.
    Params = 0b0001,
    /// Parse from url queries.
    Queries = 0b0010,
    /// Parse from headers.
    Headers = 0b0100,
    /// Parse from form.
    Form = 0b1000,
}

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
    #[inline]
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
    #[inline]
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
    pub fn header<T>(&self, key: &str) -> Option<T>
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
    pub fn cookie<T>(&self, name: T) -> Option<&Cookie<'static>>
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
    pub fn param<T>(&self, key: &str) -> Option<T>
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
    pub fn query<F>(&self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.queries().get(key).and_then(|v| v.parse::<F>().ok())
    }

    /// Get field data from form.
    #[inline]
    pub async fn form<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.form_data()
            .await
            .ok()
            .and_then(|ps| ps.fields.get(key))
            .and_then(|v| v.parse::<F>().ok())
    }
    /// Get [`FilePart`] reference from request.
    #[inline]
    pub async fn file(&mut self, key: &str) -> Option<&FilePart> {
        self.form_data().await.ok().and_then(|ps| ps.files.get(key))
    }
    /// Get [`FilePart`] reference from request.
    #[inline]
    pub async fn first_file(&mut self) -> Option<&FilePart> {
        self.form_data()
            .await
            .ok()
            .and_then(|ps| ps.files.iter().next())
            .map(|(_, f)| f)
    }
    /// Get [`FilePart`] list reference from request.
    #[inline]
    pub async fn files(&mut self, key: &str) -> Option<&Vec<FilePart>> {
        self.form_data().await.ok().and_then(|ps| ps.files.get_vec(key))
    }
    /// Get [`FilePart`] list reference from request.
    #[inline]
    pub async fn all_files(&mut self) -> Vec<&FilePart> {
        self.form_data()
            .await
            .ok()
            .map(|ps| ps.files.iter().map(|(_, f)| f).collect())
            .unwrap_or_default()
    }
    /// Get value from form first if not found then get from query.
    #[inline]
    pub async fn form_or_query<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.form(key.as_ref()).await.or_else(|| self.query(key))
    }
    /// Get value from query first if not found then get from form.
    #[inline]
    pub async fn query_or_form<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.query(key.as_ref()).or(self.form(key).await)
    }

    /// Get request payload.
    pub async fn payload(&mut self) -> Result<&Vec<u8>, ParseError> {
        let body = self.body.take();
        self.payload
            .get_or_try_init(|| async {
                match body {
                    Some(body) => hyper::body::to_bytes(body)
                        .await
                        .map(|d| d.to_vec())
                        .map_err(ParseError::Hyper),
                    None => Err(ParseError::EmptyBody),
                }
            })
            .await
    }

    /// Get `FormData` reference from request.
    #[inline]
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
                        Some(body) => FormData::read(headers, body).await,
                        None => Err(ParseError::EmptyBody),
                    }
                })
                .await
        } else {
            Err(ParseError::NotFormData)
        }
    }

    /// Read url params as type `T` from request's different sources.
    ///
    /// Returns error if the same key is appeared in different sources.
    /// This function will not handle if payload is json format, use [`pase_json`] to get typed json payload.
    #[inline]
    pub async fn parse_data<'de, T>(&'de mut self, sources: BitFlags<ParseSource>) -> Result<T, ParseError>
    where
        T: Deserialize<'de>,
    {
        if sources == ParseSource::Params {
            self.parse_params()
        } else if sources == ParseSource::Queries {
            self.parse_queries()
        } else if sources == ParseSource::Headers {
            self.parse_headers()
        } else if sources == ParseSource::Form {
            self.parse_form().await
        } else {
            let mut all_data: MultiMap<&str, &str> = MultiMap::new();
            if sources.contains(ParseSource::Form) {
                self.form_data().await?;
                if let Some(form) = self.form_data.get() {
                    if form.fields.keys().any(|key| all_data.contains_key(&**key)) {
                        return Err(ParseError::DuplicateKey);
                    }
                    for (k, v) in form.fields.iter() {
                        all_data.insert(k, v);
                    }
                }
            }
            if sources.contains(ParseSource::Params) {
                for (k, v) in self.params() {
                    all_data.insert(k, &*v);
                }
            }
            if sources.contains(ParseSource::Queries) {
                let queries = self.queries();
                if queries.keys().any(|key| all_data.contains_key(&**key)) {
                    return Err(ParseError::DuplicateKey);
                }
                for (k, v) in queries.iter() {
                    all_data.insert(k, v);
                }
            }
            if sources.contains(ParseSource::Headers) {
                let headers = self
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or_default()))
                    .collect::<HashMap<_, _>>();
                if all_data.keys().any(|key| headers.contains_key(&**key)) {
                    return Err(ParseError::DuplicateKey);
                }
                for (k, v) in headers.into_iter() {
                    all_data.insert(k, v);
                }
            }
            from_str_multi_map(all_data).map_err(ParseError::Deserialize)
        }
    }

    /// Read url params as type `T` from request.
    #[inline]
    pub fn parse_params<'de, T>(&'de mut self) -> Result<T, ParseError>
    where
        T: Deserialize<'de>,
    {
        let params = self.params().iter();
        from_str_map(params).map_err(ParseError::Deserialize)
    }

    /// Read queries as type `T` from request.
    #[inline]
    pub fn parse_queries<'de, T>(&'de mut self) -> Result<T, ParseError>
    where
        T: Deserialize<'de>,
    {
        let queries = self.queries().iter_all();
        from_str_multi_map(queries).map_err(ParseError::Deserialize)
    }

    /// Read headers as type `T` from request.
    #[inline]
    pub fn parse_headers<'de, T>(&'de mut self) -> Result<T, ParseError>
    where
        T: Deserialize<'de>,
    {
        let iter = self
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or_default()));
        from_str_map(iter).map_err(ParseError::Deserialize)
    }

    /// Read body as type `T` from request.
    #[inline]
    pub async fn parse_json<'de, T>(&'de mut self) -> Result<T, ParseError>
    where
        T: Deserialize<'de>,
    {
        self.payload()
            .await
            .and_then(|body| serde_json::from_slice::<T>(body).map_err(ParseError::SerdeJson))
    }

    /// Read body as type `T` from request.
    #[inline]
    pub async fn parse_form<'de, T>(&'de mut self) -> Result<T, ParseError>
    where
        T: Deserialize<'de>,
    {
        from_str_multi_map(self.form_data().await?.fields.iter_all()).map_err(ParseError::Deserialize)
    }

    /// Read body as type `T` from request.
    #[inline]
    pub async fn parse_body<T>(&mut self) -> Result<T, ParseError>
    where
        T: DeserializeOwned,
    {
        let ctype = self
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if ctype == "application/x-www-form-urlencoded" || ctype.starts_with("multipart/") {
            self.parse_form().await
        } else if ctype.starts_with("application/json") {
            self.parse_json().await
        } else {
            Err(ParseError::InvalidContentType)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::hyper;
    use crate::test::TestClient;

    #[tokio::test]
    async fn test_parse_queries() {
        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct BadMan<'a> {
            name: &'a str,
            age: u8,
            wives: Vec<String>,
            weapons: (u64, String, String),
        }
        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct GoodMan {
            name: String,
            age: u8,
            wives: String,
            weapons: u64,
        }
        let mut req = TestClient::get(
            "http://127.0.0.1:7979/hello?name=rust&age=25&wives=a&wives=2&weapons=69&weapons=stick&weapons=gun",
        )
        .build();
        let man = req.parse_queries::<BadMan>().unwrap();
        assert_eq!(man.name, "rust");
        assert_eq!(man.age, 25);
        assert_eq!(man.wives, vec!["a", "2"]);
        assert_eq!(man.weapons, (69, "stick".into(), "gun".into()));
        let man = req.parse_queries::<GoodMan>().unwrap();
        assert_eq!(man.name, "rust");
        assert_eq!(man.age, 25);
        assert_eq!(man.wives, "a");
        assert_eq!(man.weapons, 69);
    }

    #[tokio::test]
    async fn test_parse_json() {
        #[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
        struct User {
            name: String,
        }
        let mut req = TestClient::get("http://127.0.0.1:7878/hello")
            .json(&User { name: "jobs".into() })
            .build();
        assert_eq!(req.parse_json::<User>().await.unwrap(), User { name: "jobs".into() });
    }
    #[tokio::test]
    async fn test_query() {
        let mut req = TestClient::get("http://127.0.0.1:7979/hello?q=rust").build();
        assert_eq!(req.queries().len(), 1);
        assert_eq!(req.query::<String>("q").unwrap(), "rust");
        assert_eq!(req.query_or_form::<String>("q").await.unwrap(), "rust");
    }
    #[tokio::test]
    async fn test_form() {
        let mut req = TestClient::post("http://127.0.0.1:7979/hello?q=rust")
            .insert_header("content-type", "application/x-www-form-urlencoded")
            .raw_form("lover=dog&money=sh*t&q=firefox")
            .build();
        assert_eq!(req.form::<String>("money").await.unwrap(), "sh*t");
        assert_eq!(req.query_or_form::<String>("q").await.unwrap(), "rust");
        assert_eq!(req.form_or_query::<String>("q").await.unwrap(), "firefox");

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
        assert_eq!(req.form::<String>("money").await.unwrap(), "sh*t");
        let file = req.file("file1").await.unwrap();
        assert_eq!(file.name().unwrap(), "err.txt");
        assert_eq!(file.headers().get("content-type").unwrap(), "text/plain");
        let files = req.files("file1").await.unwrap();
        assert_eq!(files[0].name().unwrap(), "err.txt");
    }
}
