//! HTTP request.
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
#[cfg(feature = "quinn")]
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use bytes::Bytes;
#[cfg(feature = "cookie")]
use cookie::{Cookie, CookieJar};
use http::Extensions;
use http::header::{AsHeaderName, CONTENT_TYPE, HeaderMap, HeaderValue, IntoHeaderName};
use http::method::Method;
pub use http::request::Parts;
use http::uri::{Scheme, Uri};
use http_body_util::{BodyExt, Limited};
use multimap::MultiMap;
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

static GLOBAL_SECURE_MAX_SIZE: AtomicUsize = AtomicUsize::new(64 * 1024);

/// Get global secure maximum size, default value is 64KB.
///
/// **Note**: The security maximum value applies to request body reads and form parsing,
/// including multipart file uploads. Increase the limit if you need to accept large uploads.
pub fn global_secure_max_size() -> usize {
    GLOBAL_SECURE_MAX_SIZE.load(Ordering::Relaxed)
}

/// Set secure maximum size globally.
///
/// It is recommended to use the [`SecureMaxSize`] middleware to have finer-grained
/// control over [`Request`].
///
/// **Note**: The security maximum value applies to request body reads and form parsing,
/// including multipart file uploads. Increase the limit if you need to accept large uploads.
pub fn set_global_secure_max_size(size: usize) {
    GLOBAL_SECURE_MAX_SIZE.store(size, Ordering::Relaxed);
}

/// Middleware for set the secure maximum size of request body.
///
/// **Note**: The security maximum value applies to request body reads and form parsing,
/// including multipart file uploads. Increase the limit if you need to accept large uploads.
#[derive(Debug, Clone, Copy)]
pub struct SecureMaxSize(pub usize);
impl SecureMaxSize {
    /// Create a new `SecureMaxSize` instance.
    #[must_use]
    pub fn new(size: usize) -> Self {
        Self(size)
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
/// Stores all the properties of the client's request including URI, headers, body,
/// method, cookies, path parameters, query parameters, and form data.
///
/// # Body Consumption and Caching
///
/// The request body can only be read once from the underlying stream. However, Salvo
/// provides automatic caching mechanisms to allow multiple accesses:
///
/// - [`payload()`](Request::payload) and
///   [`payload_with_max_size()`](Request::payload_with_max_size) read the body and cache it
///   internally. Subsequent calls return the cached bytes.
/// - [`form_data()`](Request::form_data) parses form data and caches the result. If the body has
///   already been consumed by `payload()`, it will use the cached bytes to parse the form.
///
/// # Size Limits
///
/// To prevent denial-of-service attacks, the request body size is limited by default to 64KB.
/// This can be configured using:
///
/// - [`set_global_secure_max_size()`] - Set the global default limit
/// - [`SecureMaxSize`] middleware - Set per-route limits
/// - [`set_secure_max_size()`](Request::set_secure_max_size) - Set per-request limits
///
/// **Note**: Size limits apply to request body reads and form parsing, including
/// multipart file uploads. Increase the limit if you need to accept large uploads.
///
/// # Examples
///
/// ```
/// use salvo_core::http::Request;
///
/// // Create a new request
/// let req = Request::new();
/// assert_eq!(*req.method(), salvo_core::http::Method::GET);
/// ```
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
    fn default() -> Self {
        Self::new()
    }
}

impl Request {
    /// Creates a new blank `Request`
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
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

        Self {
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
    /// *Notice: If you using this mutable reference to change the uri, you should change the
    /// `params` and `queries` manually.*
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_core::http::*;
    /// let mut req: Request = Request::default();
    /// *req.uri_mut() = "/hello".parse().unwrap();
    /// assert_eq!(*req.uri(), *"/hello");
    /// ```
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.uri
    }

    /// Set the associated URI. `queries` will be reset.
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

    /// Returns the HTTP version of the request.
    ///
    /// Common values are `HTTP/1.0`, `HTTP/1.1`, `HTTP/2.0`, and `HTTP/3.0`.
    #[inline]
    pub fn version(&self) -> Version {
        self.version
    }

    /// Returns a mutable reference to the HTTP version.
    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.version
    }

    /// Returns the URI scheme of the request (e.g., `http` or `https`).
    #[inline]
    pub fn scheme(&self) -> &Scheme {
        &self.scheme
    }

    /// Returns a mutable reference to the URI scheme.
    #[inline]
    pub fn scheme_mut(&mut self) -> &mut Scheme {
        &mut self.scheme
    }

    /// Returns the remote (client) socket address.
    ///
    /// This is the IP address and port of the client making the request.
    /// Note that if behind a reverse proxy, this may be the proxy's address
    /// rather than the actual client's address.
    #[inline]
    pub fn remote_addr(&self) -> &SocketAddr {
        &self.remote_addr
    }

    /// Returns a mutable reference to the remote socket address.
    #[inline]
    pub fn remote_addr_mut(&mut self) -> &mut SocketAddr {
        &mut self.remote_addr
    }

    /// Returns the local (server) socket address.
    ///
    /// This is the IP address and port that the server is listening on
    /// for this connection.
    #[inline]
    pub fn local_addr(&self) -> &SocketAddr {
        &self.local_addr
    }

    /// Returns a mutable reference to the local socket address.
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
    /// req.headers_mut()
    ///     .insert(HOST, HeaderValue::from_static("world"));
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
    /// When `overwrite` is set to `true`, If the header is already present, the value will be
    /// replaced. When `overwrite` is set to `false`, The new header is always appended to the
    /// request, even if the header already exists.
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
    ///
    /// # Note
    ///
    /// The body can only be read once from the underlying stream. For most use cases,
    /// prefer using [`payload()`](Request::payload) or [`form_data()`](Request::form_data)
    /// which handle caching automatically.
    #[inline]
    pub fn body(&self) -> &ReqBody {
        &self.body
    }

    /// Returns a mutable reference to the associated HTTP body.
    ///
    /// # Note
    ///
    /// Modifying the body directly may interfere with methods like
    /// [`payload()`](Request::payload) and [`form_data()`](Request::form_data).
    /// Use with caution.
    #[inline]
    pub fn body_mut(&mut self) -> &mut ReqBody {
        &mut self.body
    }

    /// Replaces the body with a new value and returns the old body.
    ///
    /// This is useful when you need to transform or wrap the request body.
    ///
    /// # Note
    ///
    /// Replacing the body does not clear the cached payload or form data.
    /// If you've already called [`payload()`](Request::payload), those cached
    /// values will still be returned on subsequent calls.
    #[inline]
    pub fn replace_body(&mut self, body: ReqBody) -> ReqBody {
        std::mem::replace(&mut self.body, body)
    }

    /// Takes the body from the request, leaving [`ReqBody::None`] in its place.
    ///
    /// This consumes the body, making it unavailable for subsequent reads unless
    /// it was already cached via [`payload()`](Request::payload).
    ///
    /// # When to Use
    ///
    /// - When forwarding the request body to another service
    /// - When you need ownership of the body stream
    /// - When implementing custom body processing
    ///
    /// # Note
    ///
    /// Methods like [`payload()`](Request::payload) and [`form_data()`](Request::form_data)
    /// call this internally. If those methods have already been called successfully,
    /// the data is cached and can still be accessed.
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

    /// Sets the maximum allowed body size for this request.
    ///
    /// This overrides both the global default and any value set by the
    /// [`SecureMaxSize`] middleware for this specific request.
    ///
    /// # Arguments
    ///
    /// * `size` - Maximum body size in bytes.
    ///
    /// # Note
    ///
    /// This limit applies to request body reads and form parsing, including
    /// multipart file uploads. Increase the limit if you need to accept large uploads.
    pub fn set_secure_max_size(&mut self, size: usize) {
        self.secure_max_size = Some(size);
    }

    /// Returns the maximum allowed body size for this request.
    ///
    /// Returns the request-specific limit if set via
    /// [`set_secure_max_size()`](Request::set_secure_max_size) or [`SecureMaxSize`] middleware,
    /// otherwise returns the global default (64KB).
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

    /// Returns all MIME types from the `Accept` header.
    ///
    /// Parses the `Accept` header and returns a list of MIME types the client
    /// is willing to accept. Returns an empty vector if the header is missing
    /// or cannot be parsed.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // For Accept: text/html, application/json
    /// let types = req.accept();
    /// // types = [text/html, application/json]
    /// ```
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

    /// Returns the first MIME type from the `Accept` header.
    ///
    /// This is typically the client's most preferred content type.
    /// Returns `None` if the `Accept` header is missing or empty.
    #[inline]
    pub fn first_accept(&self) -> Option<Mime> {
        let mut accept = self.accept();
        if !accept.is_empty() {
            Some(accept.remove(0))
        } else {
            None
        }
    }

    /// Returns the `Content-Type` header value as a [`Mime`] type.
    ///
    /// Returns `None` if the header is missing or cannot be parsed.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if let Some(ct) = req.content_type() {
    ///     if ct.subtype() == mime::JSON {
    ///         // Handle JSON request
    ///     }
    /// }
    /// ```
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

    /// Gets a query parameter value by key, deserializing it to type `T`.
    ///
    /// Returns `None` if the key doesn't exist or deserialization fails.
    /// For error details, use [`try_query()`](Request::try_query) instead.
    ///
    /// # Type Coercion
    ///
    /// The value is deserialized from a string, so numeric types, booleans,
    /// and other `Deserialize` implementations are supported.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // URL: /search?page=2&limit=10
    /// let page: u32 = req.query("page").unwrap_or(1);
    /// let tags: Vec<String> = req.query("tags").unwrap_or_default();
    /// ```
    #[inline]
    pub fn query<'de, T>(&'de self, key: &str) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        self.try_query(key).ok()
    }

    /// Tries to get a query parameter value by key.
    ///
    /// Returns a [`ParseResult`] with either the deserialized value or an error
    /// indicating why parsing failed.
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

    /// Gets a form field value by key, deserializing it to type `T`.
    ///
    /// Returns `None` if the key doesn't exist or deserialization fails.
    /// For error details, use [`try_form()`](Request::try_form) instead.
    ///
    /// This method parses the request body as form data on first call
    /// (see [`form_data()`](Request::form_data) for caching behavior).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let username: String = req.form("username").await.unwrap_or_default();
    /// ```
    #[inline]
    pub async fn form<'de, T>(&'de mut self, key: &str) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        self.try_form(key).await.ok()
    }

    /// Tries to get a form field value by key.
    ///
    /// Returns a [`ParseResult`] with either the deserialized value or an error
    /// indicating why parsing failed.
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

    /// Gets a value from form data first, falling back to query parameters.
    ///
    /// Checks the form body for the key first. If not found, checks query parameters.
    /// Returns `None` if the key doesn't exist in either location.
    ///
    /// # Use Case
    ///
    /// Useful when a parameter can come from either a form submission or URL query string,
    /// with form data taking precedence.
    #[inline]
    pub async fn form_or_query<'de, T>(&'de mut self, key: &str) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        self.try_form_or_query(key).await.ok()
    }

    /// Tries to get a value from form data first, falling back to query parameters.
    ///
    /// Returns a [`ParseResult`] with either the deserialized value or an error.
    #[inline]
    pub async fn try_form_or_query<'de, T>(&'de mut self, key: &str) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        if let Ok(form_data) = self.form_data().await
            && form_data.fields.contains_key(key)
        {
            self.try_form(key).await
        } else {
            self.try_query(key)
        }
    }

    /// Gets a value from query parameters first, falling back to form data.
    ///
    /// Checks query parameters for the key first. If not found, checks the form body.
    /// Returns `None` if the key doesn't exist in either location.
    ///
    /// # Use Case
    ///
    /// Useful when a parameter can come from either URL query string or form submission,
    /// with query parameters taking precedence.
    #[inline]
    pub async fn query_or_form<'de, T>(&'de mut self, key: &str) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        self.try_query_or_form(key).await.ok()
    }

    /// Tries to get a value from query parameters first, falling back to form data.
    ///
    /// Returns a [`ParseResult`] with either the deserialized value or an error.
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

    /// Gets an uploaded file by form field name.
    ///
    /// Returns a [`ParseResult`] containing the first file uploaded with the given
    /// field name, or `None` if no file was uploaded with that name.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if let Some(file) = req.file("avatar").await? {
    ///     let filename = file.name().unwrap_or("unknown");
    ///     let content_type = file.content_type();
    /// }
    /// ```
    #[inline]
    pub async fn file(&mut self, key: &str) -> ParseResult<Option<&FilePart>> {
        self.form_data().await.map(|ps| ps.files.get(key))
    }

    /// Gets the first uploaded file from the request.
    ///
    /// Useful when you only expect a single file upload and don't care about the
    /// field name. Returns a [`ParseResult`] containing `None` if no files were
    /// uploaded.
    #[inline]
    pub async fn first_file(&mut self) -> ParseResult<Option<&FilePart>> {
        self.form_data()
            .await
            .map(|ps| ps.files.iter().next().map(|(_, f)| f))
    }

    /// Gets all files uploaded with the given form field name.
    ///
    /// HTML forms with `<input type="file" name="docs" multiple>` can upload
    /// multiple files under the same field name. This method returns all of them.
    ///
    /// Returns a [`ParseResult`] containing `None` if no files were uploaded with
    /// that name.
    #[inline]
    pub async fn files(&mut self, key: &str) -> ParseResult<Option<&Vec<FilePart>>> {
        self.form_data().await.map(|ps| ps.files.get_vec(key))
    }

    /// Gets all uploaded files from the request, regardless of field name.
    ///
    /// Returns a [`ParseResult`] containing an empty vector if no files were uploaded.
    #[inline]
    pub async fn all_files(&mut self) -> ParseResult<Vec<&FilePart>> {
        self.form_data()
            .await
            .map(|ps| ps.files.flat_iter().map(|(_, f)| f).collect())
    }

    /// Get request payload as raw bytes with the default size limit.
    ///
    /// Reads the entire request body into memory and caches it. The default size limit
    /// is determined by [`secure_max_size()`](Request::secure_max_size) (64KB by default).
    ///
    /// # Caching Behavior
    ///
    /// The payload is cached after the first call, so subsequent calls return the
    /// cached bytes without re-reading the body. This allows both middleware and
    /// handlers to access the raw body data.
    ///
    /// # Errors
    ///
    /// Returns an error if the body exceeds the size limit or if reading fails.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let bytes = req.payload().await?;
    /// println!("Body length: {}", bytes.len());
    /// ```
    #[inline]
    pub async fn payload(&mut self) -> ParseResult<&Bytes> {
        self.payload_with_max_size(self.secure_max_size()).await
    }

    /// Get request payload as raw bytes with a custom size limit.
    ///
    /// Similar to [`payload()`](Request::payload), but allows specifying a custom
    /// maximum size limit instead of using the default.
    ///
    /// # Arguments
    ///
    /// * `max_size` - Maximum allowed body size in bytes. If the body exceeds this limit, an error
    ///   is returned.
    ///
    /// # Caching Behavior
    ///
    /// The payload is cached after the first successful read. Once cached, the
    /// `max_size` parameter is ignored on subsequent calls since the data is
    /// already in memory.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Allow up to 1MB payload
    /// let bytes = req.payload_with_max_size(1024 * 1024).await?;
    /// ```
    #[inline]
    pub async fn payload_with_max_size(&mut self, max_size: usize) -> ParseResult<&Bytes> {
        let body = self.take_body();
        self.payload
            .get_or_try_init(|| async {
                let limited = Limited::new(body, max_size);
                let collected = limited.collect().await.map_err(|e| {
                    if e.is::<http_body_util::LengthLimitError>() {
                        ParseError::PayloadTooLarge
                    } else {
                        ParseError::other(e)
                    }
                })?;
                Ok(collected.to_bytes())
            })
            .await
    }

    /// Get [`FormData`] reference from request with the default size limit.
    ///
    /// Parses the request body as form data (either `application/x-www-form-urlencoded`
    /// or `multipart/form-data`) and caches the result for subsequent calls.
    ///
    /// Uses the default size limit from [`secure_max_size()`](Request::secure_max_size) (64KB by
    /// default). For a custom size limit, use
    /// [`form_data_max_size()`](Request::form_data_max_size).
    ///
    /// # Body Handling
    ///
    /// This method intelligently handles body consumption:
    ///
    /// - If the body hasn't been consumed yet, it reads directly from the body stream.
    /// - If the body was already consumed by [`payload()`](Request::payload), it reuses the cached
    ///   payload bytes to parse the form data.
    /// - The parsed [`FormData`] is cached, so subsequent calls return the cached result.
    ///
    /// This allows middleware to read the raw body via `payload()` while still allowing
    /// handlers to access parsed form data.
    ///
    /// # Content Type Requirements
    ///
    /// Returns [`ParseError::NotFormData`] if the `Content-Type` header is not:
    /// - `application/x-www-form-urlencoded`
    /// - `multipart/form-data`
    ///
    /// # Note
    ///
    /// For multipart form data, file uploads are written to temporary files but the
    /// overall request size is still subject to the secure max size limit.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Access form fields
    /// let form_data = req.form_data().await?;
    /// let username = form_data.fields.get("username");
    ///
    /// // Access uploaded files
    /// let file = form_data.files.get("avatar");
    /// ```
    #[inline]
    pub async fn form_data(&mut self) -> ParseResult<&FormData> {
        self.form_data_max_size(self.secure_max_size()).await
    }

    /// Get [`FormData`] reference from request with a custom size limit.
    ///
    /// Similar to [`form_data()`](Request::form_data), but allows specifying a custom
    /// maximum size limit instead of using the default.
    ///
    /// # Arguments
    ///
    /// * `max_size` - Maximum allowed body size in bytes. If the body exceeds this limit, an error
    ///   is returned.
    ///
    /// # Caching Behavior
    ///
    /// The form data is cached after the first successful parse. Once cached, the
    /// `max_size` parameter is ignored on subsequent calls since the data is
    /// already in memory.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Allow up to 1MB form data
    /// let form_data = req.form_data_max_size(1024 * 1024).await?;
    /// let username = form_data.fields.get("username");
    /// ```
    #[inline]
    pub async fn form_data_max_size(&mut self, max_size: usize) -> ParseResult<&FormData> {
        if let Some(ctype) = self.content_type() {
            if ctype.subtype() == mime::WWW_FORM_URLENCODED || ctype.type_() == mime::MULTIPART {
                let body = self.take_body();
                if body.is_none() {
                    let bytes = self.payload_with_max_size(max_size).await?.to_owned();
                    let headers = self.headers();
                    self.form_data
                        .get_or_try_init(|| async {
                            FormData::read(headers, ReqBody::Once(bytes).into_data_stream()).await
                        })
                        .await
                } else {
                    let headers = self.headers();
                    let limited = Limited::new(body, max_size);
                    self.form_data
                        .get_or_try_init(|| async {
                            FormData::read(headers, limited.into_data_stream()).await
                        })
                        .await
                }
            } else {
                Err(ParseError::NotFormData)
            }
        } else {
            Err(ParseError::NotFormData)
        }
    }

    /// Extract request as type `T` from request's different parts.
    #[inline]
    pub async fn extract<'de, T>(&'de mut self, depot: &'de mut Depot) -> ParseResult<T>
    where
        T: Extractible<'de> + Deserialize<'de> + Send,
    {
        self.extract_with_metadata(depot, T::metadata()).await
    }

    /// Extract request as type `T` from request's different parts.
    #[inline]
    pub async fn extract_with_metadata<'de, T>(
        &'de mut self,
        depot: &'de mut Depot,
        metadata: &'de Metadata,
    ) -> ParseResult<T>
    where
        T: Deserialize<'de> + Send,
    {
        from_request(self, depot, metadata).await
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

    /// Parses the JSON request body into type `T`.
    ///
    /// Uses the default size limit from [`secure_max_size()`](Request::secure_max_size).
    ///
    /// # Content Type
    ///
    /// Requires `Content-Type: application/json`. Returns [`ParseError::InvalidContentType`]
    /// if the content type doesn't match.
    ///
    /// # Empty Body Handling
    ///
    /// An empty body is treated as JSON `null`, allowing deserialization into `Option<T>`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// #[derive(Deserialize)]
    /// struct CreateUser {
    ///     name: String,
    ///     email: String,
    /// }
    ///
    /// let user: CreateUser = req.parse_json().await?;
    /// ```
    #[inline]
    pub async fn parse_json<'de, T>(&'de mut self) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        self.parse_json_with_max_size(self.secure_max_size()).await
    }

    /// Parses the JSON request body into type `T` with a custom size limit.
    ///
    /// See [`parse_json()`](Request::parse_json) for details.
    #[inline]
    pub async fn parse_json_with_max_size<'de, T>(&'de mut self, max_size: usize) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        if let Some(ctype) = self.content_type()
            && ctype.subtype() == mime::JSON
        {
            self.payload_with_max_size(max_size)
                .await
                .and_then(|payload| {
                    // fix issue https://github.com/salvo-rs/salvo/issues/545
                    let payload = if payload.is_empty() {
                        "null".as_bytes()
                    } else {
                        payload.as_ref()
                    };
                    serde_json::from_slice::<T>(payload).map_err(ParseError::SerdeJson)
                })
        } else {
            Err(ParseError::InvalidContentType)
        }
    }

    /// Parses the form request body into type `T`.
    ///
    /// Deserializes form fields into the target type. Works with both
    /// `application/x-www-form-urlencoded` and `multipart/form-data` content types.
    ///
    /// # Content Type
    ///
    /// Requires `Content-Type` to be either `application/x-www-form-urlencoded`
    /// or `multipart/form-data`. Returns [`ParseError::InvalidContentType`] otherwise.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// #[derive(Deserialize)]
    /// struct LoginForm {
    ///     username: String,
    ///     password: String,
    /// }
    ///
    /// let form: LoginForm = req.parse_form().await?;
    /// ```
    #[inline]
    pub async fn parse_form<'de, T>(&'de mut self) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        if let Some(ctype) = self.content_type()
            && (ctype.subtype() == mime::WWW_FORM_URLENCODED || ctype.subtype() == mime::FORM_DATA)
        {
            from_str_multi_map(self.form_data().await?.fields.iter_all())
                .map_err(ParseError::Deserialize)
        } else {
            Err(ParseError::InvalidContentType)
        }
    }

    /// Parses the request body as either JSON or form data into type `T`.
    ///
    /// Automatically detects the content type and parses accordingly:
    /// - `application/json` → parses as JSON
    /// - `application/x-www-form-urlencoded` or `multipart/form-data` → parses as form
    ///
    /// Uses the default size limit from [`secure_max_size()`](Request::secure_max_size).
    ///
    /// # Use Case
    ///
    /// Useful for APIs that accept both JSON and form submissions for the same endpoint.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Works with both JSON and form submissions
    /// let data: MyStruct = req.parse_body().await?;
    /// ```
    #[inline]
    pub async fn parse_body<'de, T>(&'de mut self) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        self.parse_body_with_max_size(self.secure_max_size()).await
    }

    /// Parses the request body as either JSON or form data with a custom size limit.
    ///
    /// See [`parse_body()`](Request::parse_body) for details.
    pub async fn parse_body_with_max_size<'de, T>(&'de mut self, max_size: usize) -> ParseResult<T>
    where
        T: Deserialize<'de>,
    {
        if let Some(ctype) = self.content_type() {
            if ctype.subtype() == mime::WWW_FORM_URLENCODED || ctype.subtype() == mime::FORM_DATA {
                return from_str_multi_map(
                    self.form_data_max_size(max_size).await?.fields.iter_all(),
                )
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
        let mut req = TestClient::get("http://127.0.0.1:8698/hello")
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
        let mut req = TestClient::post("http://127.0.0.1:8698/hello?q=rust")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .raw_form("lover=dog&money=sh*t&q=firefox")
            .build();
        assert_eq!(req.form::<String>("money").await.unwrap(), "sh*t");
        assert_eq!(req.query_or_form::<String>("q").await.unwrap(), "rust");
        assert_eq!(req.form_or_query::<String>("q").await.unwrap(), "firefox");

        let mut req: Request = TestClient::post("http://127.0.0.1:8698/hello?q=rust")
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
        let file = req.file("file1").await.unwrap().unwrap();
        assert_eq!(file.name().unwrap(), "err.txt");
        assert_eq!(file.headers().get("content-type").unwrap(), "text/plain");
        let files = req.files("file1").await.unwrap().unwrap();
        assert_eq!(files[0].name().unwrap(), "err.txt");
    }

    #[tokio::test]
    async fn test_file_payload_too_large_multipart() {
        let mut req = TestClient::post("http://127.0.0.1:8698/upload")
            .add_header(
                "content-type",
                "multipart/form-data; boundary=----WebKitFormBoundary7MA4YWxkTrZu0gW",
                true,
            )
            .body(
                "------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
Content-Disposition: form-data; name=\"title\"\r\n\r\nMy Document\r\n\
------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
Content-Type: text/plain\r\n\r\n\
Hello World\r\n\
------WebKitFormBoundary7MA4YWxkTrZu0gW--\r\n",
            )
            .build();
        req.set_secure_max_size(16);

        let err = req.file("file").await.unwrap_err();
        assert!(matches!(err, ParseError::PayloadTooLarge));
    }

    #[tokio::test]
    async fn test_files_payload_too_large_multipart() {
        let mut req = TestClient::post("http://127.0.0.1:8698/upload")
            .add_header(
                "content-type",
                "multipart/form-data; boundary=----WebKitFormBoundary7MA4YWxkTrZu0gW",
                true,
            )
            .body(
                "------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
Content-Disposition: form-data; name=\"title\"\r\n\r\nMy Document\r\n\
------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
Content-Type: text/plain\r\n\r\n\
Hello World\r\n\
------WebKitFormBoundary7MA4YWxkTrZu0gW--\r\n",
            )
            .build();
        req.set_secure_max_size(16);

        let err = req.files("file").await.unwrap_err();
        assert!(matches!(err, ParseError::PayloadTooLarge));
    }

    /// Test that `form_data()` works correctly after `payload()` has been accessed.
    ///
    /// This simulates a common scenario where middleware reads the request body
    /// via `payload()` before the handler tries to parse form data. The cached
    /// payload bytes should be reused by `form_data()`.
    #[tokio::test]
    async fn test_form_data_after_payload() {
        // Test 1: URL-encoded form (application/x-www-form-urlencoded)
        // Simulates browser submitting a simple HTML form
        let mut req = TestClient::post("http://127.0.0.1:8698/form")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .raw_form("username=test_user&password=secret123")
            .build();

        // Access payload first - simulates middleware reading the body
        let payload = req.payload().await.unwrap();
        assert!(!payload.is_empty());
        assert_eq!(payload.as_ref(), b"username=test_user&password=secret123");

        // form_data() should still work by using cached payload bytes
        let form_data = req.form_data().await.unwrap();
        assert_eq!(form_data.fields.get("username").unwrap(), "test_user");
        assert_eq!(form_data.fields.get("password").unwrap(), "secret123");

        // Verify form_data can be accessed multiple times (caching works)
        let form_data_again = req.form_data().await.unwrap();
        assert_eq!(form_data_again.fields.get("username").unwrap(), "test_user");

        // Test 2: Multipart form (multipart/form-data)
        // Simulates browser submitting a form with file upload
        let mut req = TestClient::post("http://127.0.0.1:8698/upload")
            .add_header(
                "content-type",
                "multipart/form-data; boundary=----WebKitFormBoundary7MA4YWxkTrZu0gW",
                true,
            )
            .body(
                "------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
Content-Disposition: form-data; name=\"title\"\r\n\r\nMy Document\r\n\
------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
Content-Type: text/plain\r\n\r\n\
Hello World\r\n\
------WebKitFormBoundary7MA4YWxkTrZu0gW--\r\n",
            )
            .build();

        // Access payload first - simulates middleware or logging reading the body
        let payload = req.payload().await.unwrap();
        assert!(!payload.is_empty());

        // form_data() should still parse multipart data from cached payload
        let form_data = req.form_data().await.unwrap();
        assert_eq!(form_data.fields.get("title").unwrap(), "My Document");

        // Verify file upload data is correctly parsed
        let file = form_data.files.get("file").unwrap();
        assert_eq!(file.name().unwrap(), "test.txt");
        assert_eq!(file.content_type().unwrap(), "text/plain");
    }

    #[tokio::test]
    async fn test_form_data_payload_too_large_urlencoded() {
        let mut req = TestClient::post("http://127.0.0.1:8698/form")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .raw_form("username=test_user&password=secret123")
            .build();
        req.set_secure_max_size(10);
        let err = req.form_data().await.unwrap_err();
        assert!(matches!(err, ParseError::PayloadTooLarge));
    }

    #[tokio::test]
    async fn test_form_data_payload_too_large_multipart() {
        let mut req = TestClient::post("http://127.0.0.1:8698/upload")
            .add_header(
                "content-type",
                "multipart/form-data; boundary=----WebKitFormBoundary7MA4YWxkTrZu0gW",
                true,
            )
            .body(
                "------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
Content-Disposition: form-data; name=\"title\"\r\n\r\nMy Document\r\n\
------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
Content-Type: text/plain\r\n\r\n\
Hello World\r\n\
------WebKitFormBoundary7MA4YWxkTrZu0gW--\r\n",
            )
            .build();
        req.set_secure_max_size(16);
        let err = req.form_data().await.unwrap_err();
        assert!(matches!(err, ParseError::PayloadTooLarge));

        let mut req = TestClient::post("http://127.0.0.1:8698/upload")
            .add_header(
                "content-type",
                "multipart/form-data; boundary=----WebKitFormBoundary7MA4YWxkTrZu0gW",
                true,
            )
            .body(
                "------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
Content-Disposition: form-data; name=\"title\"\r\n\r\nMy Document\r\n\
------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
Content-Type: text/plain\r\n\r\n\
Hello World\r\n\
------WebKitFormBoundary7MA4YWxkTrZu0gW--\r\n",
            )
            .build();
        req.set_secure_max_size(1600);
        assert!(req.form_data().await.is_ok());
    }

    #[tokio::test]
    async fn test_form_data_content_length_too_large() {
        let mut req = TestClient::post("http://127.0.0.1:8698/form")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .add_header("content-length", "9999", true)
            .raw_form("username=test_user&password=secret123")
            .build();
        req.set_secure_max_size(10);
        let err = req.form_data().await.unwrap_err();
        assert!(matches!(err, ParseError::PayloadTooLarge));
    }

    #[tokio::test]
    async fn test_parse_body_with_max_size_form() {
        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct LoginForm {
            username: String,
            password: String,
        }
        // Test that small max_size triggers error
        let mut req = TestClient::post("http://127.0.0.1:8698/form")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .raw_form("username=test_user&password=secret123")
            .build();
        let err = req
            .parse_body_with_max_size::<LoginForm>(10)
            .await
            .unwrap_err();
        assert!(matches!(err, ParseError::PayloadTooLarge));

        // Test that sufficient max_size succeeds (need new request since body was consumed)
        let mut req = TestClient::post("http://127.0.0.1:8698/form")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .raw_form("username=test_user&password=secret123")
            .build();
        assert!(
            req.parse_body_with_max_size::<LoginForm>(1000)
                .await
                .is_ok()
        );
    }
}
