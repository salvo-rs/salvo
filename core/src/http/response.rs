//! Http response.

#[cfg(feature = "cookie")]
use cookie::{Cookie, CookieJar};
use futures_util::stream::{Stream, TryStreamExt};
use http::version::Version;
use mime::Mime;
#[cfg(feature = "cookie")]
use std::borrow::Cow;
use std::collections::VecDeque;
use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};
use std::pin::Pin;
use std::task::{self, Poll};

pub use http::response::Parts;

use super::errors::*;
use super::header::{self, HeaderMap, HeaderValue, InvalidHeaderValue};
use crate::http::StatusCode;
use crate::{Error, Piece};
use bytes::Bytes;

/// Response body type.
#[allow(clippy::type_complexity)]
#[non_exhaustive]
pub enum Body {
    /// None body.
    None,
    /// Once bytes body.
    Once(Bytes),
    /// Chunks body.
    Chunks(VecDeque<Bytes>),
    /// Stream body.
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes, Box<dyn StdError + Send + Sync>>> + Send>>),
}
impl Body {
    /// Check is that body is not set.
    #[inline]
    pub fn is_none(&self) -> bool {
        matches!(*self, Body::None)
    }
}

impl Stream for Body {
    type Item = Result<Bytes, Box<dyn StdError + Send + Sync>>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Body::None => Poll::Ready(None),
            Body::Once(bytes) => {
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    let bytes = std::mem::replace(bytes, Bytes::new());
                    Poll::Ready(Some(Ok(bytes)))
                }
            }
            Body::Chunks(chunks) => Poll::Ready(chunks.pop_front().map(Ok)),
            Body::Stream(stream) => stream.as_mut().poll_next(cx),
        }
    }
}
impl From<hyper::Body> for Body {
    #[inline]
    fn from(hbody: hyper::Body) -> Body {
        Body::Stream(Box::pin(hbody.map_err(|e| e.into_cause().unwrap()).into_stream()))
    }
}

/// Represents an HTTP response
pub struct Response {
    status_code: Option<StatusCode>,
    pub(crate) status_error: Option<StatusError>,
    headers: HeaderMap,
    version: Version,
    #[cfg(feature = "cookie")]
    pub(crate) cookies: CookieJar,
    pub(crate) body: Body,
}
impl Default for Response {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
impl From<hyper::Response<hyper::Body>> for Response {
    #[inline]
    fn from(res: hyper::Response<hyper::Body>) -> Self {
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
        let cookies = if let Some(header) = headers.get(header::SET_COOKIE) {
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
            status_error: None,
            body: body.into(),
            version,
            headers,
            #[cfg(feature = "cookie")]
            cookies,
        }
    }
}
impl Response {
    /// Creates a new blank `Response`.
    #[inline]
    pub fn new() -> Response {
        Response {
            status_code: None,
            status_error: None,
            body: Body::None,
            version: Version::default(),
            headers: HeaderMap::new(),
            #[cfg(feature = "cookie")]
            cookies: CookieJar::new(),
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
    /// Set headers.
    #[inline]
    pub fn set_headers(&mut self, headers: HeaderMap) {
        self.headers = headers
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

    /// Get body reference.
    #[inline]
    pub fn body(&self) -> &Body {
        &self.body
    }
    /// Get mutable body reference.
    #[inline]
    pub fn body_mut(&mut self) -> &mut Body {
        &mut self.body
    }
    /// Set body.
    #[inline]
    pub fn set_body(&mut self, body: Body) {
        self.body = body
    }

    /// Set body to a new value and returns old value.
    #[inline]
    pub fn replace_body(&mut self, body: Body) -> Body {
        std::mem::replace(&mut self.body, body)
    }

    /// Take body from response.
    #[inline]
    pub fn take_body(&mut self) -> Body {
        std::mem::replace(&mut self.body, Body::None)
    }

    /// `write_back` is used to put all the data added to `self`
    /// back onto an `hyper::Response` so that it is sent back to the
    /// client.
    ///
    /// `write_back` consumes the `Response`.
    #[inline]
    pub(crate) async fn write_back(self, res: &mut hyper::Response<hyper::Body>) {
        let Self {
            status_code,
            #[cfg(feature = "cookie")]
            mut headers,
            #[cfg(not(feature = "cookie"))]
            headers,
            #[cfg(feature = "cookie")]
            cookies,
            body,
            ..
        } = self;
        #[cfg(feature = "cookie")]
        for cookie in cookies.delta() {
            if let Ok(hv) = cookie.encoded().to_string().parse() {
                headers.append(header::SET_COOKIE, hv);
            }
        }
        *res.headers_mut() = headers;

        // Default to a 404 if no response code was set
        *res.status_mut() = status_code.unwrap_or(StatusCode::NOT_FOUND);

        match body {
            Body::None => {
                res.headers_mut()
                    .insert(header::CONTENT_LENGTH, header::HeaderValue::from_static("0"));
            }
            Body::Once(bytes) => {
                *res.body_mut() = hyper::Body::from(bytes);
            }
            Body::Chunks(chunks) => {
                *res.body_mut() = hyper::Body::wrap_stream(tokio_stream::iter(
                    chunks.into_iter().map(Result::<_, Box<dyn StdError + Send + Sync>>::Ok),
                ));
            }
            Body::Stream(stream) => {
                *res.body_mut() = hyper::Body::wrap_stream(stream);
            }
        }
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
        pub fn add_cookie(&mut self, cookie: Cookie<'static>) {
            self.cookies.add(cookie);
        }
        /// Helper function for remove cookie.
        #[inline]
        pub fn remove_cookie<T>(&mut self, name: T)
        where
            T: Into<Cow<'static, str>>,
        {
            self.cookies.remove(Cookie::named(name));
        }
    }

    /// Get status code.
    #[inline]
    pub fn status_code(&self) -> Option<StatusCode> {
        self.status_code
    }

    /// Set status code.
    #[inline]
    pub fn set_status_code(&mut self, code: StatusCode) {
        self.status_code = Some(code);
        if !code.is_success() {
            self.status_error = StatusError::from_code(code);
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

    /// Get http error if exists, only exists after use `set_status_error` set http error.
    #[inline]
    pub fn status_error(&self) -> Option<&StatusError> {
        self.status_error.as_ref()
    }
    /// Set http error.
    #[inline]
    pub fn set_status_error(&mut self, err: StatusError) {
        self.status_code = Some(err.code);
        self.status_error = Some(err);
    }

    /// Render content.
    #[inline]
    pub fn render<P>(&mut self, piece: P)
    where
        P: Piece,
    {
        piece.render(self)
    }

    /// Render content with status code.
    #[inline]
    pub fn stuff<P>(&mut self, code: StatusCode, piece: P)
    where
        P: Piece,
    {
        self.status_code = Some(code);
        piece.render(self)
    }

    /// Write bytes data to body. If body is none, a new `Body` will created.
    #[inline]
    pub fn write_body(&mut self, data: impl Into<Bytes>) -> crate::Result<()> {
        match self.body_mut() {
            Body::None => {
                self.body = Body::Once(data.into());
            }
            Body::Once(ref bytes) => {
                let mut chunks = VecDeque::new();
                chunks.push_back(bytes.clone());
                chunks.push_back(data.into());
                self.body = Body::Chunks(chunks);
            }
            Body::Chunks(chunks) => {
                chunks.push_back(data.into());
            }
            Body::Stream(_) => {
                tracing::error!("current body kind is `Body::Stream`, try to write bytes to it");
                return Err(Error::other(
                    "current body kind is `Body::Stream`, try to write bytes to it",
                ));
            }
        }
        Ok(())
    }
    /// Write streaming data.
    #[inline]
    pub fn streaming<S, O, E>(&mut self, stream: S) -> crate::Result<()>
    where
        S: Stream<Item = Result<O, E>> + Send + 'static,
        O: Into<Bytes> + 'static,
        E: Into<Box<dyn StdError + Send + Sync>> + 'static,
    {
        match &self.body {
            Body::Once(_) => {
                return Err(Error::other("current body kind is `Body::Once` already"));
            }
            Body::Chunks(_) => {
                return Err(Error::other("current body kind is `Body::Chunks` already"));
            }
            Body::Stream(_) => {
                return Err(Error::other("current body kind is `Body::Stream` already"));
            }
            _ => {}
        }
        let mapped = stream.map_ok(Into::into).map_err(Into::into);
        self.body = Body::Stream(Box::pin(mapped));
        Ok(())
    }

    /// Redirect temporary.
    #[inline]
    pub fn redirect_temporary<U: AsRef<str>>(&mut self, url: U) {
        self.status_code = Some(StatusCode::MOVED_PERMANENTLY);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse().unwrap());
    }
    /// Redirect found.
    #[inline]
    pub fn redirect_found<U: AsRef<str>>(&mut self, url: U) {
        self.status_code = Some(StatusCode::FOUND);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse().unwrap());
    }
    /// Redirect other.
    #[inline]
    pub fn redirect_other<U: AsRef<str>>(&mut self, url: U) -> Result<(), InvalidHeaderValue> {
        self.status_code = Some(StatusCode::SEE_OTHER);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse()?);
        Ok(())
    }
}

impl fmt::Debug for Response {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        writeln!(
            f,
            "HTTP/1.1 {}\n{:?}",
            self.status_code.unwrap_or(StatusCode::NOT_FOUND),
            self.headers
        )
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
    use futures_util::stream::{iter, StreamExt};
    use std::error::Error;

    use super::*;

    #[test]
    fn test_body_empty() {
        let body = Body::Once(Bytes::from("hello"));
        assert!(!body.is_none());
        let body = Body::None;
        assert!(body.is_none());
    }

    #[tokio::test]
    async fn test_body_stream1() {
        let mut body = Body::Once(Bytes::from("hello"));

        let mut result = bytes::BytesMut::new();
        while let Some(Ok(data)) = body.next().await {
            result.extend_from_slice(&data)
        }

        assert_eq!("hello", &result)
    }

    #[tokio::test]
    async fn test_body_stream2() {
        let mut body = Body::Stream(Box::pin(iter(vec![
            Result::<_, Box<dyn Error + Send + Sync>>::Ok(BytesMut::from("Hello").freeze()),
            Result::<_, Box<dyn Error + Send + Sync>>::Ok(BytesMut::from(" World").freeze()),
        ])));

        let mut result = bytes::BytesMut::new();
        while let Some(Ok(data)) = body.next().await {
            result.extend_from_slice(&data)
        }

        assert_eq!("Hello World", &result)
    }
}
