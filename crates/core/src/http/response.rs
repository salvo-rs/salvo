//! HTTP response.
use std::collections::VecDeque;
use std::fmt::{self, Debug, Display, Formatter};
use std::path::PathBuf;

#[cfg(feature = "cookie")]
use cookie::{Cookie, CookieJar};
use futures_util::stream::Stream;
use http::header::{HeaderMap, HeaderValue, IntoHeaderName};
pub use http::response::Parts;
use http::{Extensions, version::Version};
use mime::Mime;

use crate::fs::NamedFile;
use crate::fuse::TransProto;
use crate::http::{StatusCode, StatusError};
use crate::{BoxedError, Error, Scribe};
use bytes::Bytes;

pub use crate::http::body::{BodySender, BytesFrame, ResBody};

/// Represents an HTTP response.
#[non_exhaustive]
pub struct Response {
    /// The HTTP status code.WebTransportSession
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
        #[cfg(feature = "cookie")]
        // Set the request cookies, if they exist.
        let cookies = if let Some(header) = headers.get(http::header::SET_COOKIE) {
            let mut cookie_jar = CookieJar::new();
            if let Ok(header) = header.to_str() {
                for cookie_str in header.split(';').map(|s| s.trim()) {
                    if let Ok(cookie) = Cookie::parse_encoded(cookie_str).map(|c| c.into_owned()) {
                        cookie_jar.add(cookie);
                    }
                }
            }
            cookie_jar
        } else {
            CookieJar::new()
        };

        Response {
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
    /// Creates a new blank `Response`.
    #[inline]
    pub fn new() -> Response {
        Response {
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
    pub fn with_cookies(cookies: CookieJar) -> Response {
        Response {
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

    /// If returns `true`, it means this response is ready for write back and the reset handlers should be skipped.
    #[inline]
    pub fn is_stamped(&mut self) -> bool {
        if let Some(code) = self.status_code {
            if code.is_client_error() || code.is_server_error() || code.is_redirection() {
                return true;
            }
        }
        false
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
            cookies,
            #[cfg(not(feature = "cookie"))]
            headers,
            body,
            extensions,
            ..
        } = self;

        #[cfg(feature = "cookie")]
        for cookie in cookies.delta() {
            if let Ok(hv) = cookie.encoded().to_string().parse() {
                headers.append(http::header::SET_COOKIE, hv);
            }
        }

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

    /// Strip the respone to [`hyper::Response`].
    #[doc(hidden)]
    #[inline]
    pub fn strip_to_hyper(&mut self) -> hyper::Response<ResBody> {
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
        /// Helper function for get cookie.
        #[inline]
        pub fn cookie<T>(&self, name: T) -> Option<&Cookie<'static>>
        where
            T: AsRef<str>,
        {
            self.cookies.get(name.as_ref())
        }
        /// Helper function for add cookie.
        #[inline]
        pub fn add_cookie(&mut self, cookie: Cookie<'static>)-> &mut Self {
            self.cookies.add(cookie);
            self
        }

        /// Helper function for remove cookie.
        ///
        /// Removes `cookie` from this [`CookieJar`]. If an _original_ cookie with the same
        /// name as `cookie` is present in the jar, a _removal_ cookie will be
        /// present in the `delta` computation. **To properly generate the removal
        /// cookie, `cookie` must contain the same `path` and `domain` as the cookie
        /// that was initially set.**
        ///
        /// A "removal" cookie is a cookie that has the same name as the original
        /// cookie but has an empty value, a max-age of 0, and an expiration date
        /// far in the past.
        ///
        /// Read more about [removal cookies](https://docs.rs/cookie/0.18.0/cookie/struct.CookieJar.html#method.remove).
        #[inline]
        pub fn remove_cookie(&mut self, name: &str) -> &mut Self
        {
            if let Some(cookie) = self.cookies.get(name).cloned() {
                self.cookies.remove(cookie);
            }
            self
        }
    }

    /// Get content type..
    ///
    /// # Example
    ///
    /// ```
    /// use salvo_core::http::{Response, StatusCode};
    ///
    /// let mut res = Response::new();
    /// assert_eq!(None, res.content_type());
    /// res.headers_mut().insert("content-type", "text/plain".parse().unwrap());
    /// assert_eq!(Some(mime::TEXT_PLAIN), res.content_type());
    ///
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

    /// Render content.
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

    /// Render content with status code.
    #[inline]
    pub fn stuff<P>(&mut self, code: StatusCode, scribe: P)
    where
        P: Scribe,
    {
        self.status_code = Some(code);
        scribe.render(self);
    }

    /// Attempts to send a file. If file not exists, not found error will occur.
    ///
    /// If you want more settings, you can use `NamedFile::builder` to create a new [`NamedFileBuilder`](crate::fs::NamedFileBuilder).
    pub async fn send_file<P>(&mut self, path: P, req_headers: &HeaderMap)
    where
        P: Into<PathBuf> + Send,
    {
        let path = path.into();
        if !path.exists() {
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
            ResBody::None => {
                self.body = ResBody::Once(data.into());
            }
            ResBody::Once(bytes) => {
                let mut chunks = VecDeque::new();
                chunks.push_back(bytes.clone());
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
            ResBody::Error(_) => {
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
    use bytes::BytesMut;
    use futures_util::stream::{StreamExt, iter};
    use std::error::Error;

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
}
