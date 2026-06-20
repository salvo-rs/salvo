//! HTTP response.
use std::collections::VecDeque;
use std::fmt::{self, Debug, Display, Formatter};
use std::path::PathBuf;

use bytes::Bytes;
#[cfg(feature = "cookie")]
use cookie::{Cookie, CookieJar};
use futures_util::stream::Stream;
use http::Extensions;
use http::header::{HeaderMap, HeaderValue, IntoHeaderName};
pub use http::response::Parts;
use http::version::Version;
use mime::Mime;

use crate::fs::NamedFile;
use crate::fuse::TransProto;
pub use crate::http::body::{BodySender, BytesFrame, ResBody};
use crate::http::{StatusCode, StatusError};
use crate::{BoxedError, Error, Scribe};

/// Represents an HTTP response.
///
/// # Terminal responses ("stamped")
///
/// Several parts of Salvo (notably [`FlowCtrl::call_next`] and the built-in
/// catcher logic) treat a response as **terminal** — historically called
/// *stamped* in this codebase — once a status code has been set that signals
/// the request is finished:
///
/// - any 4xx client error,
/// - any 5xx server error, or
/// - any 3xx redirection.
///
/// When that happens, the remaining handlers in the chain are skipped so a
/// downstream handler cannot accidentally overwrite the response. [`is_stamped`]
/// is the predicate used to test this condition; the name is kept for backwards
/// compatibility, but read it as "is this response in a terminal state?".
/// Successful (2xx), informational (1xx), and unset status codes are *not*
/// considered terminal.
///
/// [`FlowCtrl::call_next`]: crate::routing::FlowCtrl::call_next
/// [`is_stamped`]: Response::is_stamped
#[non_exhaustive]
pub struct Response {
    /// The HTTP status code.
    pub status_code: Option<StatusCode>,
    /// The HTTP headers.
    pub headers: HeaderMap,
    /// The HTTP version.
    pub version: Version,
    /// The HTTP cookies.
    #[cfg(feature = "cookie")]
    pub cookies: CookieJar,
    /// The HTTP body.
    pub body: ResBody,
    /// Used to store extra data derived from the underlying protocol.
    pub extensions: Extensions,
}
impl Default for Response {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
impl<B> From<hyper::Response<B>> for Response
where
    B: Into<ResBody>,
{
    #[inline]
    fn from(res: hyper::Response<B>) -> Self {
        let (
            http::response::Parts {
                status,
                version,
                headers,
                // extensions,
                ..
            },
            body,
        ) = res.into_parts();
        // Per RFC 6265 §3 each cookie is delivered in its own `Set-Cookie` header (folding
        // is forbidden), and the parts after the first `;` are attributes — not separate
        // cookies. Parse each `Set-Cookie` header value as one cookie.
        #[cfg(feature = "cookie")]
        let cookies = {
            let mut cookie_jar = CookieJar::new();
            for header in headers.get_all(http::header::SET_COOKIE) {
                if let Ok(header) = header.to_str()
                    && let Ok(cookie) = Cookie::parse_encoded(header).map(|c| c.into_owned())
                {
                    cookie_jar.add(cookie);
                }
            }
            cookie_jar
        };

        Self {
            status_code: Some(status),
            body: body.into(),
            version,
            headers,
            #[cfg(feature = "cookie")]
            cookies,
            extensions: Extensions::new(),
        }
    }
}

impl Response {
    /// Creates a new blank `Response` with the provided cookie jar.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            status_code: None,
            body: ResBody::None,
            version: Version::default(),
            headers: HeaderMap::new(),
            #[cfg(feature = "cookie")]
            cookies: CookieJar::default(),
            extensions: Extensions::new(),
        }
    }

    /// Creates a new blank `Response`.
    #[cfg(feature = "cookie")]
    #[inline]
    #[must_use]
    pub fn with_cookies(cookies: CookieJar) -> Self {
        Self {
            status_code: None,
            body: ResBody::None,
            version: Version::default(),
            headers: HeaderMap::new(),
            cookies,
            extensions: Extensions::new(),
        }
    }

    /// Get headers reference.
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    /// Get mutable headers reference.
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }
    /// Sets headers.
    #[inline]
    pub fn set_headers(&mut self, headers: HeaderMap) {
        self.headers = headers
    }

    /// Modify a header for this response.
    ///
    /// When `overwrite` is set to `true`, if the header is already present, the value will be
    /// replaced. When `overwrite` is set to `false`, the new header is always appended to the
    /// response, even if the header already exists.
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

    /// Get version.
    #[inline]
    pub fn version(&self) -> Version {
        self.version
    }
    /// Get mutable version reference.
    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.version
    }
    #[doc(hidden)]
    pub fn trans_proto(&self) -> TransProto {
        if self.version == Version::HTTP_3 {
            TransProto::Quic
        } else {
            TransProto::Tcp
        }
    }

    /// Get mutable body reference.
    #[inline]
    pub fn body_mut(&mut self) -> &mut ResBody {
        &mut self.body
    }
    /// Sets body.
    #[inline]
    pub fn body(&mut self, body: impl Into<ResBody>) -> &mut Self {
        self.body = body.into();
        self
    }

    /// Sets body to a new value and returns old value.
    #[inline]
    pub fn replace_body(&mut self, body: ResBody) -> ResBody {
        std::mem::replace(&mut self.body, body)
    }

    /// Take body from response.
    #[inline]
    pub fn take_body(&mut self) -> ResBody {
        self.replace_body(ResBody::None)
    }

    /// Returns `true` when this response is in a *terminal* state — i.e. it
    /// already has a status code that signals the request is finished, so the
    /// remaining handlers in the chain should be skipped.
    ///
    /// A response is considered terminal when its status code is set to any of:
    ///
    /// - a 4xx client error,
    /// - a 5xx server error, or
    /// - a 3xx redirection.
    ///
    /// Successful (2xx), informational (1xx), and unset status codes return
    /// `false`. The method name uses "stamped" — internal terminology for
    /// "this response has had its outcome stamped on it" — and is kept for
    /// backwards compatibility. See the [type-level docs](Response#terminal-responses-stamped)
    /// for more.
    #[inline]
    pub fn is_stamped(&self) -> bool {
        self.status_code.is_some_and(|code| {
            code.is_client_error() || code.is_server_error() || code.is_redirection()
        })
    }

    /// Append every cookie in `cookies.delta()` to `headers` as a `Set-Cookie` header,
    /// then drain the cookie jar so subsequent calls do not re-emit the same cookies.
    #[cfg(feature = "cookie")]
    fn flush_cookies_into(headers: &mut HeaderMap, cookies: &mut CookieJar) {
        for cookie in cookies.delta() {
            if let Ok(hv) = cookie.encoded().to_string().parse() {
                headers.append(http::header::SET_COOKIE, hv);
            }
        }
        // Reset the jar so a follow-up serialization does not duplicate `Set-Cookie`.
        *cookies = CookieJar::new();
    }

    /// Convert to hyper response.
    #[doc(hidden)]
    #[inline]
    pub fn into_hyper(self) -> hyper::Response<ResBody> {
        let Self {
            status_code,
            #[cfg(feature = "cookie")]
            mut headers,
            #[cfg(feature = "cookie")]
            mut cookies,
            #[cfg(not(feature = "cookie"))]
            headers,
            body,
            extensions,
            ..
        } = self;

        #[cfg(feature = "cookie")]
        Self::flush_cookies_into(&mut headers, &mut cookies);

        let status_code = status_code.unwrap_or(match &body {
            ResBody::None => StatusCode::NOT_FOUND,
            ResBody::Error(e) => e.code,
            _ => StatusCode::OK,
        });
        let mut res = hyper::Response::new(body);
        *res.extensions_mut() = extensions;
        *res.headers_mut() = headers;
        *res.status_mut() = status_code;

        res
    }

    /// Strip the response to [`hyper::Response`].
    #[doc(hidden)]
    #[inline]
    pub fn strip_to_hyper(&mut self) -> hyper::Response<ResBody> {
        // Flush any cookies onto `self.headers` *before* we transfer them to the new
        // hyper response so the tower-compat path does not silently lose `Set-Cookie`.
        #[cfg(feature = "cookie")]
        Self::flush_cookies_into(&mut self.headers, &mut self.cookies);

        let mut res = hyper::Response::new(std::mem::take(&mut self.body));
        *res.extensions_mut() = std::mem::take(&mut self.extensions);
        *res.headers_mut() = std::mem::take(&mut self.headers);
        if let Some(status) = self.status_code {
            // Default to a 404 if no response code was set
            *res.status_mut() = status;
        }

        res
    }

    /// Merge data from [`hyper::Response`].
    #[doc(hidden)]
    #[inline]
    pub fn merge_hyper<B>(&mut self, hyper_res: hyper::Response<B>)
    where
        B: Into<ResBody>,
    {
        let (
            http::response::Parts {
                status,
                version,
                headers,
                extensions,
                ..
            },
            body,
        ) = hyper_res.into_parts();

        self.status_code = Some(status);
        self.version = version;
        self.headers = headers;
        self.extensions = extensions;
        self.body = body.into();
    }

    cfg_feature! {
        #![feature = "cookie"]
        /// Get cookies reference.
        #[inline]
        pub fn cookies(&self) -> &CookieJar {
            &self.cookies
        }
        /// Get mutable cookies reference.
        #[inline]
        pub fn cookies_mut(&mut self) -> &mut CookieJar {
            &mut self.cookies
        }
        /// Gets a cookie by name from the response.
        #[inline]
        pub fn cookie<T>(&self, name: T) -> Option<&Cookie<'static>>
        where
            T: AsRef<str>,
        {
            self.cookies.get(name.as_ref())
        }
        /// Adds a cookie to the response.
        #[inline]
        pub fn add_cookie(&mut self, cookie: Cookie<'static>) -> &mut Self {
            self.cookies.add(cookie);
            self
        }

        /// Removes a cookie from the response by name.
        ///
        /// Use [`Self::remove_cookie_with`] when the removal cookie must include
        /// the same path or domain as the cookie that was initially set.
        #[inline]
        pub fn remove_cookie(&mut self, name: &str) -> &mut Self {
            if let Some(cookie) = self.cookies.get(name).cloned() {
                self.remove_cookie_with(cookie);
            } else {
                self.remove_cookie_with(Cookie::new(name.to_owned(), ""));
            }
            self
        }

        /// Removes a cookie from the response using the supplied cookie attributes.
        ///
        /// The supplied cookie is converted to a _removal_ cookie and added to
        /// the response. **To properly remove the cookie in a browser, `cookie`
        /// must contain the same `path` and `domain` as the cookie that was
        /// initially set.**
        ///
        /// A "removal" cookie is a cookie that has the same name as the original
        /// cookie but has an empty value, a max-age of 0, and an expiration date
        /// far in the past.
        ///
        /// Read more about [removal cookies](https://docs.rs/cookie/0.18.0/cookie/struct.Cookie.html#method.make_removal).
        #[inline]
        pub fn remove_cookie_with<C>(&mut self, cookie: C) -> &mut Self
        where
            C: Into<Cookie<'static>>,
        {
            let mut cookie = cookie.into();
            cookie.make_removal();
            self.cookies.add(cookie);
            self
        }
    }

    /// Returns the response `Content-Type`.
    ///
    /// # Example
    ///
    /// ```
    /// use salvo_core::http::Response;
    ///
    /// let mut res = Response::new();
    /// assert_eq!(None, res.content_type());
    /// res.headers_mut()
    ///     .insert("content-type", "text/plain".parse().unwrap());
    /// assert_eq!(Some(mime::TEXT_PLAIN), res.content_type());
    /// ```
    #[inline]
    pub fn content_type(&self) -> Option<Mime> {
        self.headers
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse().ok())
    }

    /// Sets status code and returns `&mut Self`.
    ///
    /// # Example
    ///
    /// ```
    /// use salvo_core::http::StatusCode;
    /// use salvo_core::http::response::Response;
    ///
    /// let mut res = Response::new();
    /// res.status_code(StatusCode::OK);
    /// ```
    #[inline]
    pub fn status_code(&mut self, code: StatusCode) -> &mut Self {
        self.status_code = Some(code);
        self
    }

    /// Render content into this response.
    ///
    /// Delegates to the [`Scribe`] implementation, which writes the body, headers,
    /// and (if not already set) the `Content-Type`. If serialization fails, the
    /// `Scribe` impl is responsible for converting the error into a `5xx` response
    /// instead of panicking or returning an error here.
    ///
    /// # Example
    ///
    /// ```
    /// use salvo_core::http::{Response, StatusCode};
    ///
    /// let mut res = Response::new();
    /// res.render("hello world");
    /// ```
    pub fn render<P>(&mut self, scribe: P)
    where
        P: Scribe,
    {
        scribe.render(self);
    }

    /// Sets the status code and renders content into this response.
    #[inline]
    pub fn render_with_status<P>(&mut self, code: StatusCode, scribe: P)
    where
        P: Scribe,
    {
        self.status_code = Some(code);
        scribe.render(self);
    }

    /// Sets the status code and renders content into this response.
    #[deprecated(since = "0.94.0", note = "use `Response::render_with_status` instead")]
    #[inline]
    pub fn stuff<P>(&mut self, code: StatusCode, scribe: P)
    where
        P: Scribe,
    {
        self.render_with_status(code, scribe);
    }

    /// Attempts to send a file. If file not exists, not found error will occur.
    ///
    /// If you want more settings, you can use `NamedFile::builder` to create a new
    /// [`NamedFileBuilder`](crate::fs::NamedFileBuilder).
    pub async fn send_file<P>(&mut self, path: P, req_headers: &HeaderMap)
    where
        P: Into<PathBuf> + Send,
    {
        let path = path.into();
        if tokio::fs::metadata(&path).await.is_err() {
            self.render(StatusError::not_found());
        } else {
            match NamedFile::builder(path).build().await {
                Ok(file) => file.send(req_headers, self).await,
                Err(_) => self.render(StatusError::internal_server_error()),
            }
        }
    }

    /// Write bytes data to body. If body is none, a new `ResBody` will created.
    pub fn write_body(&mut self, data: impl Into<Bytes>) -> crate::Result<()> {
        match self.body_mut() {
            ResBody::Once(bytes) => {
                let mut chunks = VecDeque::new();
                chunks.push_back(std::mem::take(bytes));
                chunks.push_back(data.into());
                self.body = ResBody::Chunks(chunks);
            }
            ResBody::Chunks(chunks) => {
                chunks.push_back(data.into());
            }
            ResBody::Hyper(_) => {
                tracing::error!(
                    "current body's kind is `ResBody::Hyper`, it is not allowed to write bytes"
                );
                return Err(Error::other(
                    "current body's kind is `ResBody::Hyper`, it is not allowed to write bytes",
                ));
            }
            ResBody::Boxed(_) => {
                tracing::error!(
                    "current body's kind is `ResBody::Boxed`, it is not allowed to write bytes"
                );
                return Err(Error::other(
                    "current body's kind is `ResBody::Boxed`, it is not allowed to write bytes",
                ));
            }
            ResBody::Stream(_) => {
                tracing::error!(
                    "current body's kind is `ResBody::Stream`, it is not allowed to write bytes"
                );
                return Err(Error::other(
                    "current body's kind is `ResBody::Stream`, it is not allowed to write bytes",
                ));
            }
            ResBody::Channel { .. } => {
                tracing::error!(
                    "current body's kind is `ResBody::Channel`, it is not allowed to write bytes"
                );
                return Err(Error::other(
                    "current body's kind is `ResBody::Channel`, it is not allowed to write bytes",
                ));
            }
            ResBody::None | ResBody::Error(_) => {
                self.body = ResBody::Once(data.into());
            }
        }
        Ok(())
    }

    /// Set response's body to stream.
    #[inline]
    pub fn stream<S, O, E>(&mut self, stream: S)
    where
        S: Stream<Item = Result<O, E>> + Send + 'static,
        O: Into<BytesFrame> + 'static,
        E: Into<BoxedError> + 'static,
    {
        self.body = ResBody::stream(stream);
    }

    /// Create a `Body` stream with an associated sender half.
    ///
    /// Useful when wanting to stream chunks from another thread.
    ///
    /// # Example
    ///
    /// ```
    /// use salvo_core::prelude::*;
    /// #[handler]
    /// async fn hello(res: &mut Response) {
    ///     res.add_header("content-type", "text/plain", true).unwrap();
    ///     let mut tx = res.channel();
    ///     tokio::spawn(async move {
    ///         tx.send_data("Hello world").await.unwrap();
    ///     });
    /// }
    /// ```
    #[inline]
    pub fn channel(&mut self) -> BodySender {
        let (sender, body) = ResBody::channel();
        self.body = body;
        sender
    }
}

impl Debug for Response {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Response")
            .field("status_code", &self.status_code)
            .field("version", &self.version)
            .field("headers", &self.headers)
            // omits Extensions because not useful
            .field("body", &self.body)
            .finish()
    }
}

impl Display for Response {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use bytes::BytesMut;
    use futures_util::stream::{StreamExt, iter};

    use super::*;

    #[test]
    fn test_body_empty() {
        let body = ResBody::Once(Bytes::from("hello"));
        assert!(!body.is_none());
        let body = ResBody::None;
        assert!(body.is_none());
    }

    #[tokio::test]
    async fn test_body_stream1() {
        let mut body = ResBody::Once(Bytes::from("hello"));

        let mut result = bytes::BytesMut::new();
        while let Some(Ok(data)) = body.next().await {
            result.extend_from_slice(&data.into_data().unwrap_or_default())
        }

        assert_eq!("hello", &result)
    }

    #[tokio::test]
    async fn test_body_stream2() {
        let mut body = ResBody::stream(iter(vec![
            Result::<_, Box<dyn Error + Send + Sync>>::Ok(BytesMut::from("Hello").freeze()),
            Result::<_, Box<dyn Error + Send + Sync>>::Ok(BytesMut::from(" World").freeze()),
        ]));

        let mut result = bytes::BytesMut::new();
        while let Some(Ok(data)) = body.next().await {
            result.extend_from_slice(&data.into_data().unwrap_or_default())
        }

        assert_eq!("Hello World", &result)
    }

    #[test]
    fn test_render_with_status_sets_status_and_body() {
        let mut res = Response::new();
        res.render_with_status(StatusCode::CREATED, "created");

        assert_eq!(res.status_code, Some(StatusCode::CREATED));
        let content_type = res.content_type().expect("content type");
        assert_eq!(content_type.essence_str(), "text/plain");
        match res.take_body() {
            ResBody::Once(bytes) => assert_eq!(bytes, Bytes::from_static(b"created")),
            body => panic!("expected once body, got {body:?}"),
        }
    }

    #[cfg(feature = "cookie")]
    #[test]
    fn test_from_hyper_response_preserves_multiple_set_cookie_headers() {
        let hyper_res = hyper::Response::builder()
            .status(StatusCode::OK)
            .header(http::header::SET_COOKIE, "sid=abc123; Path=/; HttpOnly")
            .header(http::header::SET_COOKIE, "lang=en-US; Path=/; SameSite=Lax")
            .body(ResBody::None)
            .expect("build hyper response");

        let res = Response::from(hyper_res);

        let names: Vec<_> = res.cookies.iter().map(|c| c.name().to_owned()).collect();
        assert!(
            names.contains(&"sid".to_owned()),
            "missing sid cookie: {names:?}"
        );
        assert!(
            names.contains(&"lang".to_owned()),
            "missing lang cookie: {names:?}"
        );
        // Cookie *attributes* must not be parsed as separate cookies.
        for ghost in ["Path", "HttpOnly", "SameSite"] {
            assert!(
                !names.iter().any(|n| n.eq_ignore_ascii_case(ghost)),
                "cookie attribute leaked as a cookie: {ghost} in {names:?}"
            );
        }

        // The single-cookie attributes should also be preserved.
        let sid = res.cookies.get("sid").expect("sid cookie present");
        assert_eq!(sid.value(), "abc123");
        assert_eq!(sid.http_only(), Some(true));
    }

    #[cfg(feature = "cookie")]
    #[test]
    fn test_strip_to_hyper_emits_set_cookie_for_jar_cookies() {
        use cookie::Cookie;

        let mut res = Response::new();
        res.cookies.add(Cookie::new("sid", "abc"));
        res.cookies.add(Cookie::new("theme", "dark"));

        let hyper_res = res.strip_to_hyper();
        let cookie_headers: Vec<_> = hyper_res
            .headers()
            .get_all(http::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();

        assert_eq!(
            cookie_headers.len(),
            2,
            "expected two Set-Cookie headers, got {cookie_headers:?}"
        );
        assert!(cookie_headers.iter().any(|v| v.starts_with("sid=abc")));
        assert!(cookie_headers.iter().any(|v| v.starts_with("theme=dark")));
    }

    #[cfg(feature = "cookie")]
    #[test]
    fn test_remove_cookie_emits_removal_without_original_cookie() {
        let mut res = Response::new();
        res.remove_cookie("sid");

        let hyper_res = res.strip_to_hyper();
        let cookie = hyper_res
            .headers()
            .get(http::header::SET_COOKIE)
            .expect("set-cookie header")
            .to_str()
            .expect("set-cookie should be valid");

        assert!(cookie.starts_with("sid="));
        assert!(cookie.contains("Max-Age=0"));
    }

    #[cfg(feature = "cookie")]
    #[test]
    fn test_remove_cookie_with_emits_path_and_domain_without_original_cookie() {
        use cookie::Cookie;

        let mut res = Response::new();
        res.remove_cookie_with(
            Cookie::build("sid")
                .path("/app")
                .domain("example.com")
                .build(),
        );

        let hyper_res = res.strip_to_hyper();
        let cookie = hyper_res
            .headers()
            .get(http::header::SET_COOKIE)
            .expect("set-cookie header")
            .to_str()
            .expect("set-cookie should be valid");

        assert!(cookie.starts_with("sid="));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("Path=/app"));
        assert!(cookie.contains("Domain=example.com"));
    }
}
