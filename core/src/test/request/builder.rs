use std::borrow::Borrow;
use std::convert::{From, TryInto};
use std::fs;
use std::str;
use std::time::Duration;

use http::header::{
    HeaderMap, HeaderValue, IntoHeaderName, ACCEPT, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING,
    USER_AGENT,
};
use http::{Method, Uri};
use hyper::Body;
use url::Url;

use crate::test::{Error, Result};


/// `RequestBuilder` is the main way of building requests.
///
/// You can create a `RequestBuilder` using the `new` or `try_new` method, but the recommended way
/// or use one of the simpler constructors available in the crate root or on the `Session` struct,
/// such as `get`, `post`, etc.
#[derive(Debug)]
pub struct RequestBuilder {
    url: Url,
    method: Method,
    headers: HeaderMap,
    body: Body,
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
        let url = Url::parse(url.as_ref()).map_err(|_| Error::InvalidUrl)?;
        Ok(Self {
            url,
            method,
            headers: HeaderMap::new(),
            body: Body::default(),
        })
    }
}

impl RequestBuilder {
    /// Associate a query string parameter to the given value.
    ///
    /// The same key can be used multiple times.
    pub fn query<K, V>(mut self, key: K, value: V) -> Self
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
    pub fn queries<P, K, V>(mut self, pairs: P) -> Self
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
        let mut encoded = String::from("Basic ");
        base64::encode_config_buf(auth.as_bytes(), base64::STANDARD, &mut encoded);
        self.insert_header(http::header::AUTHORIZATION, encoded)
    }

    /// Enable HTTP bearer authentication.
    pub fn bearer_auth(self, token: impl Into<String>) -> Self {
        self.insert_header(http::header::AUTHORIZATION, format!("Bearer {}", token.into()))
    }

    /// Set the body of this request.
    pub fn body(self, body: impl Into<Body>) -> Self {
        self.body = body.into();
        self
    }

    /// Set the body of this request to be text.
    ///
    /// If the `Content-Type` header is unset, it will be set to `text/plain` and the charset to UTF-8.
    pub fn text(mut self, body: impl Into<String>) -> Self {
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("text/plain; charset=utf-8"));
        self.body(body.into())
    }

    /// Set the body of this request to be bytes.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/octet-stream`.
    pub fn bytes(mut self, body: Vec<u8>) -> Self {
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/octet-stream"));
        self.body(body)
    }

    /// Set the body of this request using a local file.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/octet-stream`.
    // pub fn file(mut self, body: fs::File) -> RequestBuilder {
    //     self.headers
    //         .entry(http::header::CONTENT_TYPE)
    //         .or_insert(HeaderValue::from_static("application/octet-stream"));
    //     self.body(body::File(body))
    // }

    /// Set the body of this request to be the JSON representation of the given object.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/json` and the charset to UTF-8.
    pub fn json<T: serde::Serialize>(mut self, value: &T) -> Result<Self> {
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json; charset=utf-8"));
        Ok(self.body(serde_json::to_vec(value)?))
    }

    /// Set the body of this request to be the URL-encoded representation of the given object.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/x-www-form-urlencoded`.
    pub fn form<T: serde::Serialize>(mut self, value: &T) -> Self {
        let body = serde_urlencoded::to_string(value).unwrap().into_bytes();
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/x-www-form-urlencoded"));
        Ok(self.body(body))
    }

    /// Modify a header for this request.
    ///
    /// If the header is already present, the value will be replaced. If you wish to append a new header,
    /// use `header_append`.
    pub fn insert_header<H, V>(mut self, header: H, value: V) -> Self
    where
        H: IntoHeaderName,
        V: TryInto<HeaderValue>,
        Error: From<V::Error>,
    {
        Ok(self.headers.insert(header, value.try_into().unwrap()))
    }

    /// Append a new header to this request.
    ///
    /// The new header is always appended to the request, even if the header already exists.
    pub fn append_header<H, V>(mut self, header: H, value: V) -> Self
    where
        H: IntoHeaderName,
        V: TryInto<HeaderValue>,
        Error: From<V::Error>,
    {
        Ok(self.headers.append(header, value.try_into().unwrap()))
    }

    // pub async fn send(r: R) -> RequestedData {}
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
            ],
            &[],
        );
    }

    #[test]
    fn test_prepare_default_headers() {
        let prepped = RequestBuilder::new(Method::GET, "http://localhost:1337/foo/qux/baz").prepare();
        assert_eq!(prepped.headers()[ACCEPT], "*/*");
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
