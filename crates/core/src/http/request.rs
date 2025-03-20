//! HTTP request.
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
#[cfg(feature = "quinn")]
use std::sync::Arc;
use std::sync::OnceLock;

use bytes::Bytes;
#[cfg(feature = "cookie")]
use cookie::{Cookie, CookieJar};
use http::Extensions;
use http::header::{AsHeaderName, CONTENT_TYPE, HeaderMap, HeaderValue, IntoHeaderName};
use http::method::Method;
use http::uri::{Scheme, Uri};

pub use http::request::Parts;

use http_body_util::{BodyExt, Limited};
use multimap::MultiMap;
use parking_lot::RwLock;
use serde::de::Deserialize;

use crate::conn::SocketAddr;
use crate::extract::{Extractible, Metadata};
use crate::fuse::TransProto;
use crate::http::body::ReqBody;
use crate::http::form::{FilePart, FormData};
use crate::http::{Mime, ParseError, ParseResult, Response, Version};
use crate::routing::PathParams;
use crate::serde::{
    from_request, from_str_map, from_str_multi_map, from_str_multi_val, from_str_val,
};
use crate::{Depot, Error, FlowCtrl, Handler, async_trait};

static GLOBAL_SECURE_MAX_SIZE: RwLock<usize> = RwLock::new(64 * 1024);

/// Get global secure maximum size, default value is 64KB.
///
/// **Note**: The security maximum value is only effective when directly obtaining data
/// from the body. For uploaded files, the files are written to temporary files
/// and the bytes is not directly obtained, so they will not be affected.
pub fn global_secure_max_size() -> usize {
    *GLOBAL_SECURE_MAX_SIZE.read()
}

/// Set secure maximum size globally.
///
/// It is recommended to use the [`SecureMaxSize`] middleware to have finer-grained
/// control over [`Request`].
///
/// **Note**: The security maximum value is only effective when directly obtaining data
/// from the body. For uploaded files, the files are written to temporary files
/// and the bytes is not directly obtained, so they will not be affected.
pub fn set_global_secure_max_size(size: usize) {
    let mut lock = GLOBAL_SECURE_MAX_SIZE.write();
    *lock = size;
}

/// Middleware for set the secure maximum size of request body.
///
/// **Note**: The security maximum value is only effective when directly obtaining data
/// from the body. For uploaded files, the files are written to temporary files
/// and the bytes is not directly obtained, so they will not be affected.
pub struct SecureMaxSize(pub usize);
impl SecureMaxSize {
    /// Create a new `SecureMaxSize` instance.
    pub fn new(size: usize) -> Self {
        SecureMaxSize(size)
    }
}
#[async_trait]
impl Handler for SecureMaxSize {
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        _res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        req.secure_max_size = Some(self.0);
    }
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
    pub(crate) body: ReqBody,
    pub(crate) extensions: Extensions,

    // The request method.
    method: Method,

    #[cfg(feature = "cookie")]
    pub(crate) cookies: CookieJar,

    pub(crate) params: PathParams,

    pub(crate) queries: OnceLock<MultiMap<String, String>>,
    pub(crate) form_data: tokio::sync::OnceCell<FormData>,
    pub(crate) payload: tokio::sync::OnceCell<Bytes>,

    /// The version of the HTTP protocol used.
    pub(crate) version: Version,
    pub(crate) scheme: Scheme,
    pub(crate) local_addr: SocketAddr,
    pub(crate) remote_addr: SocketAddr,

    pub(crate) secure_max_size: Option<usize>,
    #[cfg(feature = "matched-path")]
    pub(crate) matched_path: String,
}

impl Debug for Request {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Request")
            .field("method", self.method())
            .field("uri", self.uri())
            .field("version", &self.version())
            .field("scheme", &self.scheme())
            .field("headers", self.headers())
            // omits Extensions because not useful
            .field("body", &self.body())
            .field("local_addr", &self.local_addr)
            .field("remote_addr", &self.remote_addr)
            .finish()
    }
}

impl Default for Request {
    #[inline]
    fn default() -> Request {
        Request::new()
    }
}

impl Request {
    /// Creates a new blank `Request`
    #[inline]
    pub fn new() -> Request {
        Request {
            uri: Uri::default(),
            headers: HeaderMap::default(),
            body: ReqBody::default(),
            extensions: Extensions::default(),
            method: Method::default(),
            #[cfg(feature = "cookie")]
            cookies: CookieJar::default(),
            params: PathParams::new(),
            queries: OnceLock::new(),
            form_data: tokio::sync::OnceCell::new(),
            payload: tokio::sync::OnceCell::new(),
            version: Version::default(),
            scheme: Scheme::HTTP,
            local_addr: SocketAddr::Unknown,
            remote_addr: SocketAddr::Unknown,
            secure_max_size: None,
            #[cfg(feature = "matched-path")]
            matched_path: Default::default(),
        }
    }
    #[doc(hidden)]
    pub fn trans_proto(&self) -> TransProto {
        if self.version == Version::HTTP_3 {
            TransProto::Quic
        } else {
            TransProto::Tcp
        }
    }
    /// Creates a new `Request` from [`hyper::Request`].
    pub fn from_hyper<B>(req: hyper::Request<B>, scheme: Scheme) -> Self
    where
        B: Into<ReqBody>,
    {
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
        #[cfg(feature = "cookie")]
        let cookies = {
            let mut cookie_jar = CookieJar::new();
            for header in headers.get_all(http::header::COOKIE) {
                if let Ok(header) = header.to_str() {
                    for cookie_str in header.split(';').map(|s| s.trim()) {
                        if let Ok(cookie) =
                            Cookie::parse_encoded(cookie_str).map(|c| c.into_owned())
                        {
                            cookie_jar.add_original(cookie);
                        }
                    }
                }
            }
            cookie_jar
        };

        Request {
            queries: OnceLock::new(),
            uri,
            headers,
            body: body.into(),
            extensions,
            method,
            #[cfg(feature = "cookie")]
            cookies,
            // accept: None,
            params: PathParams::new(),
            form_data: tokio::sync::OnceCell::new(),
            payload: tokio::sync::OnceCell::new(),
            // multipart: OnceLock::new(),
            local_addr: SocketAddr::Unknown,
            remote_addr: SocketAddr::Unknown,
            version,
            scheme,
            secure_max_size: None,
            #[cfg(feature = "matched-path")]
            matched_path: Default::default(),
        }
    }

    /// Strip the request to [`hyper::Request`].
    #[doc(hidden)]
    pub fn strip_to_hyper<QB>(&mut self) -> Result<hyper::Request<QB>, crate::Error>
    where
        QB: TryFrom<ReqBody>,
        <QB as TryFrom<ReqBody>>::Error: StdError + Send + Sync + 'static,
    {
        let mut builder = http::request::Builder::new()
            .method(self.method.clone())
            .uri(self.uri.clone())
            .version(self.version);
        if let Some(headers) = builder.headers_mut() {
            *headers = std::mem::take(&mut self.headers);
        }
        if let Some(extensions) = builder.extensions_mut() {
            *extensions = std::mem::take(&mut self.extensions);
        }

        std::mem::take(&mut self.body)
            .try_into()
            .map_err(crate::Error::other)
            .and_then(|body| builder.body(body).map_err(crate::Error::other))
    }

    /// Merge data from [`hyper::Request`].
    #[doc(hidden)]
    pub fn merge_hyper(&mut self, hyper_req: hyper::Request<ReqBody>) {
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
        ) = hyper_req.into_parts();

        self.method = method;
        self.uri = uri;
        self.version = version;
        self.headers = headers;
        self.extensions = extensions;
        self.body = body;
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
    /// *Notice: If you using this mutable reference to change the uri, you should change the `params` and `queries` manually.*
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

    /// Set the associated URI. `querie` will be reset.
    ///
    /// *Notice: `params` will not reset.*
    #[inline]
    pub fn set_uri(&mut self, uri: Uri) {
        self.uri = uri;
        self.queries = OnceLock::new();
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

    /// Returns the associated scheme.
    #[inline]
    pub fn scheme(&self) -> &Scheme {
        &self.scheme
    }
    /// Returns a mutable reference to the associated scheme.
    #[inline]
    pub fn scheme_mut(&mut self) -> &mut Scheme {
        &mut self.scheme
    }

    /// Get request remote address.
    #[inline]
    pub fn remote_addr(&self) -> &SocketAddr {
        &self.remote_addr
    }
    /// Get request remote address.
    #[inline]
    pub fn remote_addr_mut(&mut self) -> &mut SocketAddr {
        &mut self.remote_addr
    }

    /// Get request local address reference.
    #[inline]
    pub fn local_addr(&self) -> &SocketAddr {
        &self.local_addr
    }
    /// Get mutable request local address reference.
    #[inline]
    pub fn local_addr_mut(&mut self) -> &mut SocketAddr {
        &mut self.local_addr
    }

    cfg_feature! {
        #![feature = "matched-path"]

        /// Get matched path.
        #[inline]
        pub fn matched_path(&self) -> &str {
            &self.matched_path
        }
        /// Get mutable matched path.
        #[inline]
        pub fn matched_path_mut(&mut self) -> &mut String {
            &mut self.matched_path
        }
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

    /// Get header with supplied name and try to parse to a 'T'.
    ///
    /// Returns `None` if failed or not found.
    #[inline]
    pub fn header<'de, T>(&'de self, key: impl AsHeaderName) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        self.try_header(key).ok()
    }

    /// Try to get header with supplied name and try to parse to a 'T'.
    #[inline]
    pub fn try_header<'de, T>(&'de self, key: impl AsHeaderName) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        let values = self
            .headers
            .get_all(key)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect::<Vec<_>>();
        from_str_multi_val(values).map_err(Into::into)
    }

    /// Modify a header for this request.
    ///
    /// When `overwrite` is set to `true`, If the header is already present, the value will be replaced.
    /// When `overwrite` is set to `false`, The new header is always appended to the request, even if the header already exists.
    pub fn add_header<N, V>(
        &mut self,
        name: N,
        value: V,
        overwrite: bool,
    ) -> crate::Result<&mut Self>
    where
        N: IntoHeaderName,
        V: TryInto<HeaderValue>,
    {
        let value = value
            .try_into()
            .map_err(|_| Error::Other("invalid header value".into()))?;
        if overwrite {
            self.headers.insert(name, value);
        } else {
            self.headers.append(name, value);
        }
        Ok(self)
    }

    /// Returns a reference to the associated HTTP body.
    #[inline]
    pub fn body(&self) -> &ReqBody {
        &self.body
    }
    /// Returns a mutable reference to the associated HTTP body.
    #[inline]
    pub fn body_mut(&mut self) -> &mut ReqBody {
        &mut self.body
    }

    /// Sets body to a new value and returns old value.
    #[inline]
    pub fn replace_body(&mut self, body: ReqBody) -> ReqBody {
        std::mem::replace(&mut self.body, body)
    }

    /// Take body form the request, and set the body to None in the request.
    #[inline]
    pub fn take_body(&mut self) -> ReqBody {
        self.replace_body(ReqBody::None)
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

    /// Set secure max size, default value is 64KB.
    pub fn set_secure_max_size(&mut self, size: usize) {
        self.secure_max_size = Some(size);
    }

    /// Get secure max size, default value is 64KB.
    pub fn secure_max_size(&self) -> usize {
        self.secure_max_size.unwrap_or_else(global_secure_max_size)
    }

    cfg_feature! {
        #![feature = "quinn"]

        #[inline]
        fn is_wt_connect(&self) -> bool {
            let protocol = self.extensions().get::<salvo_http3::ext::Protocol>();
            matches!((self.method(), protocol), (&Method::CONNECT, Some(p)) if p == &salvo_http3::ext::Protocol::WEB_TRANSPORT)
        }

        /// Try to get a WebTransport session from the request.
        pub async fn web_transport_mut(&mut self) -> Result<&mut crate::proto::WebTransportSession<salvo_http3::quinn::Connection, Bytes>, crate::Error> {
            if self.is_wt_connect() {
                if self.extensions.get::<crate::proto::WebTransportSession<salvo_http3::quinn::Connection, Bytes>>().is_none() {
                    let conn = self.extensions.remove::<Arc<std::sync::Mutex<salvo_http3::server::Connection<salvo_http3::quinn::Connection, Bytes>>>>();
                    let stream = self.extensions.remove::<Arc<salvo_http3::server::RequestStream<salvo_http3::quinn::BidiStream<Bytes>, Bytes>>>();
                    match (conn, stream) {
                        (Some(conn), Some(stream)) => {
                            if let Some(conn) = Arc::into_inner(conn) {
                                if let Ok(conn) = conn.into_inner() {
                                    if let Some(stream) = Arc::into_inner(stream) {
                                        let session =  crate::proto::WebTransportSession::accept(stream, conn).await?;
                                        self.extensions.insert(Arc::new(session));
                                        if let Some(session) = self.extensions.get_mut::<Arc<crate::proto::WebTransportSession<salvo_http3::quinn::Connection, Bytes>>>() {
                                            if let Some(session) = Arc::get_mut(session) {
                                                Ok(session)
                                            } else {
                                                Err(crate::Error::Other("web transport session should not used twice".into()))
                                            }
                                        } else {
                                            Err(crate::Error::Other("web transport session not found in request extension".into()))
                                        }
                                    } else {
                                        Err(crate::Error::Other("web transport stream should not used twice".into()))
                                    }
                                } else {
                                    Err(crate::Error::Other("invalid web transport".into()))
                                }
                            } else {
                                Err(crate::Error::Other("quinn connection should not used twice".into()))
                            }
                        }
                        (Some(conn), None) => {
                            self.extensions_mut().insert(Arc::new(conn));
                            Err(crate::Error::Other("invalid web transport without stream".into()))
                        }
                        (None, Some(stream)) => {
                            self.extensions_mut().insert(Arc::new(stream));
                            Err(crate::Error::Other("invalid web transport without connection".into()))
                        }
                        (None, None) => Err(crate::Error::Other("invalid web transport without connection and stream".into())),
                    }
                } else if let Some(session) = self.extensions.get_mut::<crate::proto::WebTransportSession<salvo_http3::quinn::Connection, Bytes>>() {
                    Ok(session)
                } else {
                    Err(crate::Error::Other("invalid web transport".into()))
                }
            } else {
                Err(crate::Error::Other("no web transport".into()))
            }
        }
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
    pub fn first_accept(&self) -> Option<Mime> {
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
            .get(CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse().ok())
    }

    cfg_feature! {
        #![feature = "cookie"]
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
    }
    /// Get params reference.
    #[inline]
    pub fn params(&self) -> &PathParams {
        &self.params
    }
    /// Get params mutable reference.
    #[inline]
    pub fn params_mut(&mut self) -> &mut PathParams {
        &mut self.params
    }

    /// Get param value from params.
    #[inline]
    pub fn param<'de, T>(&'de self, key: &str) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        self.try_param(key).ok()
    }

    /// Try to get param value from params.
    #[inline]
    pub fn try_param<'de, T>(&'de self, key: &str) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        self.params
            .get(key)
            .ok_or(ParseError::NotExist)
            .and_then(|v| from_str_val(v).map_err(Into::into))
    }

    /// Get queries reference.
    pub fn queries(&self) -> &MultiMap<String, String> {
        self.queries.get_or_init(|| {
            form_urlencoded::parse(self.uri.query().unwrap_or_default().as_bytes())
                .into_owned()
                .collect()
        })
    }
    /// Get mutable queries reference.
    pub fn queries_mut(&mut self) -> &mut MultiMap<String, String> {
        let _ = self.queries();
        self.queries
            .get_mut()
            .expect("queries should be initialized")
    }

    /// Get query value from queries.
    #[inline]
    pub fn query<'de, T>(&'de self, key: &str) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        self.try_query(key).ok()
    }

    /// Try to get query value from queries.
    #[inline]
    pub fn try_query<'de, T>(&'de self, key: &str) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        self.queries()
            .get_vec(key)
            .ok_or(ParseError::NotExist)
            .and_then(|vs| from_str_multi_val(vs).map_err(Into::into))
    }

    /// Get field data from form.
    #[inline]
    pub async fn form<'de, T>(&'de mut self, key: &str) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        self.try_form(key).await.ok()
    }

    /// Try to get field data from form.
    #[inline]
    pub async fn try_form<'de, T>(&'de mut self, key: &str) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        self.form_data()
            .await
            .and_then(|ps| ps.fields.get_vec(key).ok_or(ParseError::NotExist))
            .and_then(|vs| from_str_multi_val(vs).map_err(Into::into))
    }

    /// Get field data from form, if key is not found in form data, then get from query.
    #[inline]
    pub async fn form_or_query<'de, T>(&'de mut self, key: &str) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        self.try_form_or_query(key).await.ok()
    }

    /// Try to get field data from form, if key is not found in form data, then get from query.
    #[inline]
    pub async fn try_form_or_query<'de, T>(&'de mut self, key: &str) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        if let Ok(form_data) = self.form_data().await {
            if form_data.fields.contains_key(key) {
                return self.try_form(key).await;
            }
        }
        self.try_query(key)
    }

    /// Get value from query, if key is not found in queries, then get from form.
    #[inline]
    pub async fn query_or_form<'de, T>(&'de mut self, key: &str) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        self.try_query_or_form(key).await.ok()
    }

    /// Try to get value from query, if key is not found in queries, then get from form.
    #[inline]
    pub async fn try_query_or_form<'de, T>(&'de mut self, key: &str) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        if self.queries().contains_key(key) {
            self.try_query(key)
        } else {
            self.try_form(key).await
        }
    }

    /// Get [`FilePart`] reference from request.
    #[inline]
    pub async fn file(&mut self, key: &str) -> Option<&FilePart> {
        self.try_file(key).await.ok().flatten()
    }
    /// Try to get [`FilePart`] reference from request.
    #[inline]
    pub async fn try_file(&mut self, key: &str) -> ParseResult<Option<&FilePart>> {
        self.form_data().await.map(|ps| ps.files.get(key))
    }

    /// Get [`FilePart`] reference from request.
    #[inline]
    pub async fn first_file(&mut self) -> Option<&FilePart> {
        self.try_first_file().await.ok().flatten()
    }

    /// Try to get [`FilePart`] reference from request.
    #[inline]
    pub async fn try_first_file(&mut self) -> ParseResult<Option<&FilePart>> {
        self.form_data()
            .await
            .map(|ps| ps.files.iter().next().map(|(_, f)| f))
    }

    /// Get [`FilePart`] list reference from request.
    #[inline]
    pub async fn files(&mut self, key: &str) -> Option<&Vec<FilePart>> {
        self.try_files(key).await.ok().flatten()
    }
    /// Try to get [`FilePart`] list reference from request.
    #[inline]
    pub async fn try_files(&mut self, key: &str) -> ParseResult<Option<&Vec<FilePart>>> {
        self.form_data().await.map(|ps| ps.files.get_vec(key))
    }

    /// Get [`FilePart`] list reference from request.
    #[inline]
    pub async fn all_files(&mut self) -> Vec<&FilePart> {
        self.try_all_files().await.unwrap_or_default()
    }

    /// Try to get [`FilePart`] list reference from request.
    #[inline]
    pub async fn try_all_files(&mut self) -> ParseResult<Vec<&FilePart>> {
        self.form_data()
            .await
            .map(|ps| ps.files.flat_iter().map(|(_, f)| f).collect())
    }

    /// Get request payload with default max size limit(64KB).
    ///
    /// <https://github.com/hyperium/hyper/issues/3111>
    /// *Notice: This method takes body.
    #[inline]
    pub async fn payload(&mut self) -> ParseResult<&Bytes> {
        self.payload_with_max_size(self.secure_max_size()).await
    }

    /// Get request payload with max size limit.
    ///
    /// <https://github.com/hyperium/hyper/issues/3111>
    /// *Notice: This method takes body.
    #[inline]
    pub async fn payload_with_max_size(&mut self, max_size: usize) -> ParseResult<&Bytes> {
        let body = self.take_body();
        self.payload
            .get_or_try_init(|| async {
                Ok(Limited::new(body, max_size)
                    .collect()
                    .await
                    .map_err(ParseError::other)?
                    .to_bytes())
            })
            .await
    }

    /// Get `FormData` reference from request.
    ///
    /// *Notice: This method takes body and body's size is not limited.
    #[inline]
    pub async fn form_data(&mut self) -> ParseResult<&FormData> {
        if let Some(ctype) = self.content_type() {
            if ctype.subtype() == mime::WWW_FORM_URLENCODED || ctype.type_() == mime::MULTIPART {
                let body = self.take_body();
                let headers = self.headers();
                self.form_data
                    .get_or_try_init(|| async { FormData::read(headers, body).await })
                    .await
            } else {
                Err(ParseError::NotFormData)
            }
        } else {
            Err(ParseError::NotFormData)
        }
    }

    /// Extract request as type `T` from request's different parts.
    #[inline]
    pub async fn extract<'de, T>(&'de mut self) -> ParseResult<T>
    where
        T: Extractible<'de> + Deserialize<'de> + Send,
    {
        self.extract_with_metadata(T::metadata()).await
    }

    /// Extract request as type `T` from request's different parts.
    #[inline]
    pub async fn extract_with_metadata<'de, T>(
        &'de mut self,
        metadata: &'de Metadata,
    ) -> ParseResult<T>
    where
        T: Deserialize<'de> + Send,
    {
        from_request(self, metadata).await
    }

    /// Parse url params as type `T` from request.
    #[inline]
    pub fn parse_params<'de, T>(&'de mut self) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        let params = self.params().iter();
        from_str_map(params).map_err(ParseError::Deserialize)
    }

    /// Parse queries as type `T` from request.
    #[inline]
    pub fn parse_queries<'de, T>(&'de mut self) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        let queries = self.queries().iter_all();
        from_str_multi_map(queries).map_err(ParseError::Deserialize)
    }

    /// Parse headers as type `T` from request.
    #[inline]
    pub fn parse_headers<'de, T>(&'de mut self) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        let iter = self
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or_default()));
        from_str_map(iter).map_err(ParseError::Deserialize)
    }

    cfg_feature! {
        #![feature = "cookie"]
        /// Parse cookies as type `T` from request.
        #[inline]
        pub fn parse_cookies<'de, T>(&'de mut self) -> ParseResult<T>
        where
            T: Deserialize<'de>,
        {
            let iter = self
                .cookies()
                .iter()
                .map(|c| c.name_value());
            from_str_map(iter).map_err(ParseError::Deserialize)
        }
    }

    /// Parse json body as type `T` from request with default max size limit.
    #[inline]
    pub async fn parse_json<'de, T>(&'de mut self) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        self.parse_json_with_max_size(self.secure_max_size()).await
    }
    /// Parse json body as type `T` from request with max size limit.
    #[inline]
    pub async fn parse_json_with_max_size<'de, T>(&'de mut self, max_size: usize) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        let ctype = self.content_type();
        if let Some(ctype) = ctype {
            if ctype.subtype() == mime::JSON {
                return self
                    .payload_with_max_size(max_size)
                    .await
                    .and_then(|payload| {
                        // fix issue https://github.com/salvo-rs/salvo/issues/545
                        let payload = if payload.is_empty() {
                            "null".as_bytes()
                        } else {
                            payload.as_ref()
                        };
                        serde_json::from_slice::<T>(payload).map_err(ParseError::SerdeJson)
                    });
            }
        }
        Err(ParseError::InvalidContentType)
    }

    /// Parse form body as type `T` from request.
    #[inline]
    pub async fn parse_form<'de, T>(&'de mut self) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        if let Some(ctype) = self.content_type() {
            if ctype.subtype() == mime::WWW_FORM_URLENCODED || ctype.subtype() == mime::FORM_DATA {
                return from_str_multi_map(self.form_data().await?.fields.iter_all())
                    .map_err(ParseError::Deserialize);
            }
        }
        Err(ParseError::InvalidContentType)
    }

    /// Parse json body or form body as type `T` from request with default max size.
    #[inline]
    pub async fn parse_body<'de, T>(&'de mut self) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        self.parse_body_with_max_size(self.secure_max_size()).await
    }

    /// Parse json body or form body as type `T` from request with max size.
    pub async fn parse_body_with_max_size<'de, T>(&'de mut self, max_size: usize) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        if let Some(ctype) = self.content_type() {
            if ctype.subtype() == mime::WWW_FORM_URLENCODED || ctype.subtype() == mime::FORM_DATA {
                return from_str_multi_map(self.form_data().await?.fields.iter_all())
                    .map_err(ParseError::Deserialize);
            } else if ctype.subtype() == mime::JSON {
                return self.payload_with_max_size(max_size).await.and_then(|body| {
                    serde_json::from_slice::<T>(body).map_err(ParseError::SerdeJson)
                });
            }
        }
        Err(ParseError::InvalidContentType)
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
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
            "http://127.0.0.1:5801/hello?name=rust&age=25&wives=a&wives=2&weapons=69&weapons=stick&weapons=gun",
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
        let mut req = TestClient::get("http://127.0.0.1:5800/hello")
            .json(&User {
                name: "jobs".into(),
            })
            .build();
        assert_eq!(
            req.parse_json::<User>().await.unwrap(),
            User {
                name: "jobs".into()
            }
        );
    }
    #[tokio::test]
    async fn test_query() {
        let req = TestClient::get(
            "http://127.0.0.1:5801/hello?name=rust&name=25&name=a&name=2&weapons=98&weapons=gun",
        )
        .build();
        assert_eq!(req.queries().len(), 2);
        assert_eq!(req.query::<String>("name").unwrap(), "rust");
        assert_eq!(req.query::<&str>("name").unwrap(), "rust");
        assert_eq!(req.query::<i64>("weapons").unwrap(), 98);
        let names = req.query::<Vec<&str>>("name").unwrap();
        let weapons = req.query::<(u64, &str)>("weapons").unwrap();
        assert_eq!(names, vec!["rust", "25", "a", "2"]);
        assert_eq!(weapons, (98, "gun"));
    }
    #[tokio::test]
    async fn test_form() {
        let mut req = TestClient::post("http://127.0.0.1:5800/hello?q=rust")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .raw_form("lover=dog&money=sh*t&q=firefox")
            .build();
        assert_eq!(req.form::<String>("money").await.unwrap(), "sh*t");
        assert_eq!(req.query_or_form::<String>("q").await.unwrap(), "rust");
        assert_eq!(req.form_or_query::<String>("q").await.unwrap(), "firefox");

        let mut req: Request = TestClient::post("http://127.0.0.1:5800/hello?q=rust")
            .add_header(
                "content-type",
                "multipart/form-data; boundary=----WebKitFormBoundary0mkL0yrNNupCojyz",
                true,
            )
            .body(
                "------WebKitFormBoundary0mkL0yrNNupCojyz\r\n\
Content-Disposition: form-data; name=\"money\"\r\n\r\nsh*t\r\n\
------WebKitFormBoundary0mkL0yrNNupCojyz\r\n\
Content-Disposition: form-data; name=\"file1\"; filename=\"err.txt\"\r\n\
Content-Type: text/plain\r\n\r\n\
file content\r\n\
------WebKitFormBoundary0mkL0yrNNupCojyz--\r\n",
            )
            .build();
        assert_eq!(req.form::<String>("money").await.unwrap(), "sh*t");
        let file = req.file("file1").await.unwrap();
        assert_eq!(file.name().unwrap(), "err.txt");
        assert_eq!(file.headers().get("content-type").unwrap(), "text/plain");
        let files = req.files("file1").await.unwrap();
        assert_eq!(files[0].name().unwrap(), "err.txt");
    }
}
