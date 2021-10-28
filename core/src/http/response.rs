use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt::{self, Debug};
use std::io::{self, Write};
use std::pin::Pin;
use std::task::{self, Poll};

use async_compression::tokio::bufread::{BrotliDecoder, DeflateDecoder, GzipDecoder};
use bytes::{BufMut, Bytes, BytesMut};
use cookie::{Cookie, CookieJar};
use encoding_rs::{Encoding, UTF_8};
use futures_util::stream::{Stream, StreamExt, TryStreamExt};
use http::version::Version;
use mime::Mime;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncReadExt, BufReader};

pub use http::response::Parts;

use super::errors::*;
use super::header::{self, HeaderMap, HeaderValue, InvalidHeaderValue, CONTENT_ENCODING, SET_COOKIE};
use crate::http::StatusCode;

#[allow(clippy::type_complexity)]
pub enum Body {
    Empty,
    Bytes(BytesMut),
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes, Box<dyn StdError + Send + Sync>>> + Send>>),
}
impl Body {
    pub fn is_empty(&self) -> bool {
        matches!(*self, Body::Empty)
    }
    pub async fn take_json<T: DeserializeOwned>(&mut self) -> crate::Result<T> {
        let full = self.take_bytes().await?;
        serde_json::from_slice(&full).map_err(crate::Error::new)
    }
    pub async fn take_text(&mut self, charset: &str, compress: Option<&str>) -> crate::Result<String> {
        let charset = Encoding::for_label(charset.as_bytes()).unwrap_or(UTF_8);
        let mut full = self.take_bytes().await?;
        if let Some(algo) = compress {
            match algo {
                "gzip" => {
                    let mut reader = GzipDecoder::new(BufReader::new(full.as_ref()));
                    let mut buf = vec![];
                    reader.read_to_end(&mut buf).await.map_err(crate::Error::new)?;
                    full = Bytes::from(buf);
                }
                "deflate" => {
                    let mut reader = DeflateDecoder::new(BufReader::new(full.as_ref()));
                    let mut buf = vec![];
                    reader.read_to_end(&mut buf).await.map_err(crate::Error::new)?;
                    full = Bytes::from(buf);
                }
                "br" => {
                    let mut reader = BrotliDecoder::new(BufReader::new(full.as_ref()));
                    let mut buf = vec![];
                    reader.read_to_end(&mut buf).await.map_err(crate::Error::new)?;
                    full = Bytes::from(buf);
                }
                _ => {
                    tracing::error!(compress = %algo, "unknown compress format");
                }
            }
        }
        let (text, _, _) = charset.decode(&full);
        if let Cow::Owned(s) = text {
            return Ok(s);
        }
        unsafe {
            // decoding returned Cow::Borrowed, meaning these bytes
            // are already valid utf8
            Ok(String::from_utf8_unchecked(full.to_vec()))
        }
    }
    pub async fn take_bytes(&mut self) -> crate::Result<Bytes> {
        let bytes = match self {
            Self::Empty => Bytes::new(),
            Self::Bytes(bytes) => std::mem::take(bytes).freeze(),
            Self::Stream(stream) => {
                let mut bytes = BytesMut::new();
                while let Some(chunk) = stream.next().await {
                    bytes.extend(chunk.map_err(crate::Error::new)?);
                }
                bytes.freeze()
            }
        };
        Ok(bytes)
    }
}

impl Stream for Body {
    type Item = Result<Bytes, Box<dyn StdError + Send + Sync>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Body::Empty => Poll::Ready(None),
            Body::Bytes(bytes) => {
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Ok(bytes.split().freeze())))
                }
            }
            Body::Stream(stream) => {
                let x = stream.as_mut();
                x.poll_next(cx)
            }
        }
    }
}
impl From<hyper::Body> for Body {
    fn from(hbody: hyper::Body) -> Body {
        Body::Stream(Box::pin(hbody.map_err(|e| e.into_cause().unwrap()).into_stream()))
    }
}
/// Represents an HTTP response
pub struct Response {
    /// The response status-code.
    status_code: Option<StatusCode>,
    pub(crate) http_error: Option<HttpError>,
    /// The headers of the response.
    headers: HeaderMap,
    version: Version,
    pub(crate) cookies: CookieJar,
    pub(crate) body: Option<Body>,
    is_committed: bool,
}
impl Default for Response {
    fn default() -> Self {
        Self::new()
    }
}
impl From<hyper::Response<hyper::Body>> for Response {
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

        // Set the request cookies, if they exist.
        let cookies = if let Some(header) = headers.get(header::SET_COOKIE) {
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

        Response {
            status_code: Some(status),
            http_error: None,
            body: Some(body.into()),
            version,
            headers,
            cookies,
            is_committed: false,
        }
    }
}
impl Response {
    /// Creates a new blank `Response`.
    pub fn new() -> Response {
        Response {
            status_code: None,
            http_error: None,
            body: None,
            version: Version::default(),
            headers: HeaderMap::new(),
            cookies: CookieJar::new(),
            is_committed: false,
        }
    }

    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }
    #[inline]
    pub fn set_headers(&mut self, headers: HeaderMap) {
        self.headers = headers
    }
    #[inline]
    pub fn version(&self) -> Version {
        self.version
    }

    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.version
    }

    #[inline]
    pub fn body(&self) -> Option<&Body> {
        self.body.as_ref()
    }
    #[inline]
    pub fn body_mut(&mut self) -> Option<&mut Body> {
        self.body.as_mut()
    }
    #[inline]
    pub fn set_body(&mut self, body: Option<Body>) {
        self.body = body
    }
    #[inline]
    pub fn take_body(&mut self) -> Option<Body> {
        self.body.take()
    }
    pub async fn take_json<T: DeserializeOwned>(&mut self) -> crate::Result<T> {
        match &mut self.body {
            Some(body) => body.take_json().await,
            None => Err(crate::Error::new("body is none")),
        }
    }
    pub async fn take_text(&mut self) -> crate::Result<String> {
        self.take_text_with_charset("utf-8").await
    }
    pub async fn take_text_with_charset(&mut self, default_charset: &str) -> crate::Result<String> {
        match &mut self.body {
            Some(body) => {
                let content_type = self
                    .headers
                    .get(header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<Mime>().ok());
                let charset = content_type
                    .as_ref()
                    .and_then(|mime| mime.get_param("charset").map(|charset| charset.as_str()))
                    .unwrap_or(default_charset);
                body.take_text(
                    charset,
                    self.headers.get(CONTENT_ENCODING).and_then(|v| v.to_str().ok()),
                )
                .await
            }
            None => Err(crate::Error::new("body is none")),
        }
    }
    pub async fn take_bytes(&mut self) -> crate::Result<Bytes> {
        match &mut self.body {
            Some(body) => body.take_bytes().await,
            None => Err(crate::Error::new("body is none")),
        }
    }

    // `write_back` is used to put all the data added to `self`
    // back onto an `hyper::Response` so that it is sent back to the
    // client.
    //
    // `write_back` consumes the `Response`.
    pub(crate) async fn write_back(self, res: &mut hyper::Response<hyper::Body>) {
        *res.headers_mut() = self.headers;

        // Default to a 404 if no response code was set
        *res.status_mut() = self.status_code.unwrap_or(StatusCode::NOT_FOUND);

        if let Some(body) = self.body {
            match body {
                Body::Bytes(bytes) => {
                    *res.body_mut() = hyper::Body::from(Bytes::from(bytes));
                }
                Body::Stream(stream) => {
                    *res.body_mut() = hyper::Body::wrap_stream(stream);
                }
                _ => {
                    res.headers_mut()
                        .insert(header::CONTENT_LENGTH, header::HeaderValue::from_static("0"));
                }
            }
        } else {
            res.headers_mut()
                .insert(header::CONTENT_LENGTH, header::HeaderValue::from_static("0"));
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
    pub fn status_code(&self) -> Option<StatusCode> {
        self.status_code
    }

    #[inline]
    pub fn set_status_code(&mut self, code: StatusCode) {
        let is_success = code.is_success();
        self.status_code = Some(code);
        if !is_success {
            self.commit();
        }
    }
    // #[inline]
    // pub fn content_type(&self) -> Option<Mime> {
    //     self.headers.get_one("Content-Type").and_then(|v| v.parse().ok())
    // }

    #[inline]
    pub fn http_error(&self) -> Option<&HttpError> {
        self.http_error.as_ref()
    }
    #[inline]
    pub fn set_http_error(&mut self, err: HttpError) {
        self.status_code = Some(err.code);
        self.http_error = Some(err);
        self.commit();
    }

    pub fn render_json<T: Serialize>(&mut self, data: &T) {
        let mut cache = Cache::with_capacity(128);
        match serde_json::to_writer(&mut cache, data) {
            Ok(_) => {
                self.headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json; charset=utf-8"),
                );
                self.set_body(Some(Body::Bytes(cache.into_inner())));
            }
            Err(_) => self.set_http_error(InternalServerError().with_summary("error when serialize object to json")),
        }
    }

    pub fn render_json_text(&mut self, data: &str) {
        self.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        self.set_body(Some(Body::Bytes(BytesMut::from(data))));
    }

    #[inline]
    pub fn render_html_text(&mut self, data: &str) {
        self.render_binary(HeaderValue::from_static("text/html; charset=utf-8"), data.as_bytes());
    }
    #[inline]
    pub fn render_plain_text(&mut self, data: &str) {
        self.render_binary(HeaderValue::from_static("text/plain; charset=utf-8"), data.as_bytes());
    }
    #[inline]
    pub fn render_xml_text(&mut self, data: &str) {
        self.render_binary(HeaderValue::from_static("text/xml; charset=utf-8"), data.as_bytes());
    }
    // RenderBinary renders store from memory (which could be a file that has not been written,
    // the output from some function, or bytes streamed from somewhere else, as long
    // it implements io.Reader).  When called directly on something generated or
    // streamed, modtime should mostly likely be time.Now().
    #[inline]
    pub fn render_binary(&mut self, content_type: HeaderValue, data: &[u8]) {
        self.headers.insert(header::CONTENT_TYPE, content_type);
        self.write_body_bytes(data);
    }

    #[inline]
    pub fn write_body_bytes(&mut self, data: &[u8]) {
        if let Some(body) = self.body_mut() {
            match body {
                Body::Bytes(bytes) => {
                    bytes.extend_from_slice(data);
                }
                Body::Stream(_) => {
                    tracing::error!("current body kind is stream, try to write bytes to it");
                }
                _ => {
                    self.body = Some(Body::Bytes(BytesMut::from(data)));
                }
            }
        } else {
            self.body = Some(Body::Bytes(BytesMut::from(data)));
        }
    }
    #[inline]
    pub fn streaming<S, O, E>(&mut self, stream: S)
    where
        S: Stream<Item = Result<O, E>> + Send + 'static,
        O: Into<Bytes> + 'static,
        E: Into<Box<dyn StdError + Send + Sync>> + 'static,
    {
        if let Some(body) = &self.body {
            match body {
                Body::Bytes(_) => {
                    tracing::warn!("Current body kind is bytes already");
                }
                Body::Stream(_) => {
                    tracing::warn!("Current body kind is stream already");
                }
                _ => {}
            }
        }
        let mapped = stream.map_ok(Into::into).map_err(Into::into);
        self.body = Some(Body::Stream(Box::pin(mapped)));
    }

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
            self.headers
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse().unwrap());
        self.commit();
    }
    #[inline]
    pub fn redirect_other<U: AsRef<str>>(&mut self, url: U) -> Result<(), InvalidHeaderValue> {
        self.status_code = Some(StatusCode::SEE_OTHER);
        if !self.headers().contains_key(header::CONTENT_TYPE) {
            self.headers
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
        }
        self.headers.insert(header::LOCATION, url.as_ref().parse()?);
        self.commit();
        Ok(())
    }

    /// Salvo executes before handler and path handler in sequence, when the response is in a
    /// committed state, subsequent handlers will not be executed, and then all after
    /// handlers will be executed.
    ///
    /// This is a sign that the http request is completed, which can be used to process early
    /// return verification logic, such as permission verification, etc.
    #[inline]
    pub fn commit(&mut self) {
        if self.is_committed {
            return;
        }
        for cookie in self.cookies.delta() {
            if let Ok(hv) = cookie.encoded().to_string().parse() {
                self.headers.append(SET_COOKIE, hv);
            }
        }
        self.is_committed = true;
    }
    #[inline]
    pub fn is_committed(&self) -> bool {
        self.is_committed
    }
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

pub(crate) struct Cache(BytesMut);

impl Cache {
    pub fn with_capacity(size: usize) -> Self {
        Cache(BytesMut::with_capacity(size))
    }

    pub fn into_inner(self) -> BytesMut {
        self.0
    }
}

impl Write for Cache {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.put(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use bytes::BytesMut;
    use cookie::Cookie;
    use futures_util::stream::{iter, StreamExt};
    use serde::Deserialize;
    use std::error::Error;

    use super::*;

    #[test]
    fn test_body_empty() {
        let body = Body::Bytes(BytesMut::from("hello"));
        assert!(!body.is_empty());
        let body = Body::Empty;
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn test_body_stream1() {
        let mut body = Body::Bytes(BytesMut::from("hello"));

        let mut result = bytes::BytesMut::new();
        while let Some(Ok(data)) = body.next().await {
            result.extend_from_slice(&data)
        }

        assert_eq!("hello", &result)
    }

    #[tokio::test]
    async fn test_body_stream2() {
        let mut body = Body::Stream(Box::pin(iter(vec![
            Result::<_, Box<dyn Error + Send + Sync>>::Ok(BytesMut::from("hello").freeze()),
            Result::<_, Box<dyn Error + Send + Sync>>::Ok(BytesMut::from(" world").freeze()),
        ])));

        let mut result = bytes::BytesMut::new();
        while let Some(Ok(data)) = body.next().await {
            result.extend_from_slice(&data)
        }

        assert_eq!("hello world", &result)
    }
    #[tokio::test]
    async fn test_others() {
        let mut response: Response = hyper::Response::builder()
            .header("set-cookie", "lover=dog")
            .body("response body".into())
            .unwrap()
            .into();
        assert_eq!(response.header_cookies().len(), 1);
        response.cookies_mut().add(Cookie::new("money", "sh*t"));
        assert_eq!(response.cookies().get("money").unwrap().value(), "sh*t");
        response.commit();
        assert_eq!(response.header_cookies().len(), 2);
        assert_eq!(response.take_bytes().await.unwrap().len(), b"response body".len());

        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct User {
            name: String,
        }

        let mut response: Response = hyper::Response::builder()
            .body(r#"{"name": "jobs"}"#.into())
            .unwrap()
            .into();
        assert_eq!(
            response.take_json::<User>().await.unwrap(),
            User { name: "jobs".into() }
        );
    }
}
