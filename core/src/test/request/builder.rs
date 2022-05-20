use std::borrow::Borrow;
use std::convert::{From, TryInto};
use std::fs;
use std::str;
use std::time::Duration;

use http::{
    header::{
        HeaderMap, HeaderValue, IntoHeaderName, ACCEPT, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING,
        USER_AGENT,
    },
    Method,
};
use url::Url;

use super::body::{self, Body, BodyKind};
use super::{header_append, header_insert, header_insert_if_missing, BaseSettings, PreparedRequest};

use crate::error::{Error, ErrorKind, Result};

const DEFAULT_USER_AGENT: &str = concat!("salvo/", env!("CARGO_PKG_VERSION"));

/// `RequestBuilder` is the main way of building requests.
///
/// You can create a `RequestBuilder` using the `new` or `try_new` method, but the recommended way
/// or use one of the simpler constructors available in the crate root or on the `Session` struct,
/// such as `get`, `post`, etc.
#[derive(Debug)]
pub struct RequestBuilder<B = body::Empty> {
    url: Url,
    method: Method,
    headers: HeaderMap,
    body: B,
}

impl RequestBuilder {
    /// Create a new `RequestBuilder` with the base URL and the given method.
    ///
    /// # Panics
    /// Panics if the base url is invalid or if the method is CONNECT.
    pub fn new<U>(url: U, method: Method) -> Self
    where
        U: AsRef<str>,
    {
        Self::try_new(url, method).expect("invalid url or method")
    }

    /// Try to create a new `RequestBuilder`.
    ///
    /// If the base URL is invalid, an error is returned.
    /// If the method is CONNECT, an error is also returned. CONNECT is not yet supported.
    pub fn try_new<U>(url: U, method: Method) -> Result<Self>
    where
        U: AsRef<str>,
    {
        let url = Url::parse(url.as_ref()).map_err(|_| ErrorKind::InvalidBaseUrl)?;

        if method == Method::CONNECT {
            return Err(ErrorKind::ConnectNotSupported.into());
        }

        Ok(Self {
            url,
            method,
            headers: HeaderMap::new(),
            body: body::Empty,
        })
    }
}

impl<B> RequestBuilder<B> {
    /// Associate a query string parameter to the given value.
    ///
    /// The same key can be used multiple times.
    pub fn param<K, V>(mut self, key: K, value: V) -> Self
    where
        K: AsRef<str>,
        V: ToString,
    {
        self.url.query_pairs_mut().append_pair(key.as_ref(), &value.to_string());
        self
    }

    /// Associated a list of pairs to query parameters.
    ///
    /// The same key can be used multiple times.
    ///
    /// # Example
    /// ```
    /// attohttpc::get("http://foo.bar").params(&[("p1", "v1"), ("p2", "v2")]);
    /// ```
    pub fn params<P, K, V>(mut self, pairs: P) -> Self
    where
        P: IntoIterator,
        P::Item: Borrow<(K, V)>,
        K: AsRef<str>,
        V: ToString,
    {
        for pair in pairs.into_iter() {
            let (key, value) = pair.borrow();
            self.url.query_pairs_mut().append_pair(key.as_ref(), &value.to_string());
        }
        self
    }

    /// Enable HTTP basic authentication.
    pub fn basic_auth(self, username: impl std::fmt::Display, password: Option<impl std::fmt::Display>) -> Self {
        let auth = match password {
            Some(password) => format!("{}:{}", username, password),
            None => format!("{}:", username),
        };
        self.insert_header(
            http::header::AUTHORIZATION,
            format!("Basic {}", base64::encode_block(auth.as_bytes())),
        )
    }

    /// Enable HTTP bearer authentication.
    pub fn bearer_auth(self, token: impl Into<String>) -> Self {
        self.insert_header(http::header::AUTHORIZATION, format!("Bearer {}", token.into()))
    }

    /// Set the body of this request.
    ///
    /// The [BodyKind enum](crate::body::BodyKind) and [Body trait](crate::body::Body)
    /// determine how to implement custom request body types.
    pub fn body<B1: Body>(self, body: B1) -> RequestBuilder<B1> {
        RequestBuilder {
            url: self.url,
            method: self.method,
            headers: self.headers,
            body,
        }
    }

    /// Set the body of this request to be text.
    ///
    /// If the `Content-Type` header is unset, it will be set to `text/plain` and the charset to UTF-8.
    pub fn text<B1: AsRef<str>>(mut self, body: B1) -> RequestBuilder<body::Text<B1>> {
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("text/plain; charset=utf-8"));
        self.body(body::Text(body))
    }

    /// Set the body of this request to be bytes.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/octet-stream`.
    pub fn bytes<B1: AsRef<[u8]>>(mut self, body: B1) -> RequestBuilder<body::Bytes<B1>> {
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/octet-stream"));
        self.body(body::Bytes(body))
    }

    /// Set the body of this request using a local file.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/octet-stream`.
    pub fn file(mut self, body: fs::File) -> RequestBuilder<body::File> {
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/octet-stream"));
        self.body(body::File(body))
    }

    /// Set the body of this request to be the JSON representation of the given object.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/json` and the charset to UTF-8.
    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize>(mut self, value: &T) -> Result<RequestBuilder<body::Bytes<Vec<u8>>>> {
        let body = serde_json::to_vec(value)?;
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json; charset=utf-8"));
        Ok(self.body(body::Bytes(body)))
    }

    /// Set the body of this request to stream out a JSON representation of the given object.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/json` and the charset to UTF-8.
    #[cfg(feature = "json")]
    pub fn json_streaming<T: serde::Serialize>(mut self, value: T) -> RequestBuilder<body::Json<T>> {
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json; charset=utf-8"));
        self.body(body::Json(value))
    }

    /// Set the body of this request to be the URL-encoded representation of the given object.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/x-www-form-urlencoded`.
    #[cfg(feature = "form")]
    pub fn form<T: serde::Serialize>(mut self, value: &T) -> Result<RequestBuilder<body::Bytes<Vec<u8>>>> {
        let body = serde_urlencoded::to_string(value)?.into_bytes();
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/x-www-form-urlencoded"));
        Ok(self.body(body::Bytes(body)))
    }

    /// Modify a header for this request.
    ///
    /// If the header is already present, the value will be replaced. If you wish to append a new header,
    /// use `header_append`.
    pub fn insert_header<H, V>(mut self, header: H, value: V) -> Result<Self>
    where
        H: IntoHeaderName,
        V: TryInto<HeaderValue>,
        Error: From<V::Error>,
    {
        Ok(self.headers.insert(header, value.try_into()?))
    }

    /// Append a new header to this request.
    ///
    /// The new header is always appended to the request, even if the header already exists.
    pub fn append_header<H, V>(mut self, header: H, value: V) -> Result<Self>
    where
        H: IntoHeaderName,
        V: TryInto<HeaderValue>,
        Error: From<V::Error>,
    {
        Ok(self.headers.append(header, value.try_into()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::header::HeaderMap;

    #[test]
    fn test_header_insert_exists() {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("hello"));
        header_insert(&mut headers, USER_AGENT, "world").unwrap();
        assert_eq!(headers[USER_AGENT], "world");
    }

    #[test]
    fn test_header_insert_missing() {
        let mut headers = HeaderMap::new();
        header_insert(&mut headers, USER_AGENT, "world").unwrap();
        assert_eq!(headers[USER_AGENT], "world");
    }

    #[test]
    fn test_header_insert_if_missing_exists() {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("hello"));
        header_insert_if_missing(&mut headers, USER_AGENT, "world").unwrap();
        assert_eq!(headers[USER_AGENT], "hello");
    }

    #[test]
    fn test_header_insert_if_missing_missing() {
        let mut headers = HeaderMap::new();
        header_insert_if_missing(&mut headers, USER_AGENT, "world").unwrap();
        assert_eq!(headers[USER_AGENT], "world");
    }

    #[test]
    fn test_header_append() {
        let mut headers = HeaderMap::new();
        header_append(&mut headers, USER_AGENT, "hello").unwrap();
        header_append(&mut headers, USER_AGENT, "world").unwrap();

        let vals: Vec<_> = headers.get_all(USER_AGENT).into_iter().collect();
        assert_eq!(vals.len(), 2);
        for val in vals {
            assert!(val == "hello" || val == "world");
        }
    }

    #[test]
    fn test_request_builder_param() {
        let prepped = RequestBuilder::new(Method::GET, "http://localhost:1337/foo")
            .param("qux", "baz")
            .prepare();

        assert_eq!(prepped.url().as_str(), "http://localhost:1337/foo?qux=baz");
    }

    #[test]
    fn test_request_builder_params() {
        let prepped = RequestBuilder::new(Method::GET, "http://localhost:1337/foo")
            .params(&[("qux", "baz"), ("foo", "bar")])
            .prepare();

        assert_eq!(prepped.url().as_str(), "http://localhost:1337/foo?qux=baz&foo=bar");
    }

    #[test]
    fn test_request_builder_header_insert() {
        let prepped = RequestBuilder::new(Method::GET, "http://localhost:1337/foo")
            .header("hello", "world")
            .prepare();

        assert_eq!(prepped.headers()["hello"], "world");
    }

    #[test]
    fn test_request_builder_header_append() {
        let prepped = RequestBuilder::new(Method::GET, "http://localhost:1337/foo")
            .header_append("hello", "world")
            .header_append("hello", "!!!")
            .prepare();

        let vals: Vec<_> = prepped.headers().get_all("hello").into_iter().collect();
        assert_eq!(vals.len(), 2);
        for val in vals {
            assert!(val == "world" || val == "!!!");
        }
    }

    #[cfg(feature = "compress")]
    fn assert_request_content(
        builder: RequestBuilder,
        status_line: &str,
        mut header_lines: Vec<&str>,
        body_lines: &[&str],
    ) {
        let mut buf = Vec::new();

        let mut prepped = builder.prepare();
        prepped
            .write_request(&mut buf, &prepped.url().clone(), None)
            .expect("error writing request");

        let text = std::str::from_utf8(&buf).expect("cannot decode request as utf-8");
        let lines: Vec<_> = text.lines().collect();

        let req_status_line = lines[0];

        let empty_line_pos = lines
            .iter()
            .position(|l| l.is_empty())
            .expect("no empty line in request");
        let mut req_header_lines = lines[1..empty_line_pos].to_vec();

        let req_body_lines = &lines[empty_line_pos + 1..];

        req_header_lines.sort_unstable();
        header_lines.sort_unstable();

        assert_eq!(req_status_line, status_line);
        assert_eq!(req_header_lines, header_lines);
        assert_eq!(req_body_lines, body_lines);
    }

    #[test]
    #[cfg(feature = "compress")]
    fn test_request_builder_write_request_no_query() {
        assert_request_content(
            RequestBuilder::new(Method::GET, "http://localhost:1337/foo"),
            "GET /foo HTTP/1.1",
            vec![
                "connection: close",
                "accept-encoding: gzip, deflate",
                "accept: */*",
                &format!("user-agent: {}", DEFAULT_USER_AGENT),
            ],
            &[],
        );
    }

    #[test]
    #[cfg(feature = "compress")]
    fn test_request_builder_write_request_with_query() {
        assert_request_content(
            RequestBuilder::new(Method::GET, "http://localhost:1337/foo").param("hello", "world"),
            "GET /foo?hello=world HTTP/1.1",
            vec![
                "connection: close",
                "accept-encoding: gzip, deflate",
                "accept: */*",
                &format!("user-agent: {}", DEFAULT_USER_AGENT),
            ],
            &[],
        );
    }

    #[test]
    fn test_prepare_default_headers() {
        let prepped = RequestBuilder::new(Method::GET, "http://localhost:1337/foo/qux/baz").prepare();
        assert_eq!(prepped.headers()[ACCEPT], "*/*");
        assert_eq!(prepped.headers()[USER_AGENT], DEFAULT_USER_AGENT);
    }

    #[test]
    fn test_prepare_custom_headers() {
        let prepped = RequestBuilder::new(Method::GET, "http://localhost:1337/foo/qux/baz")
            .header(USER_AGENT, "foobaz")
            .header("Accept", "nothing")
            .prepare();
        assert_eq!(prepped.headers()[ACCEPT], "nothing");
        assert_eq!(prepped.headers()[USER_AGENT], "foobaz");
    }
}
