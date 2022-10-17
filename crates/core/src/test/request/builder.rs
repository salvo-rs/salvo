use std::borrow::Borrow;
use std::convert::{From, TryInto};
use std::str;
use std::sync::Arc;

use http::header::{self, HeaderMap, HeaderValue, IntoHeaderName};
use hyper::{Body, Method};
use url::Url;

use crate::{async_trait, Depot, Error, FlowCtrl, Handler, Request, Response, Router, Service};

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
    // params: HashMap<String, String>,
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
        let url = Url::parse(url.as_ref()).unwrap();
        Self {
            url,
            method,
            headers: HeaderMap::new(),
            // params: HeaderMap::new(),
            body: Body::default(),
        }
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
    /// ```ignore
    /// TestClient::get("http://foo.bar").queries(&[("p1", "v1"), ("p2", "v2")]);
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

    // /// Associate a url param to the given value.
    // pub fn param<K, V>(mut self, key: K, value: V) -> Self
    // where
    //     K: AsRef<str>,
    //     V: ToString,
    // {
    //     self.params.insert(key.as_ref(), &value.to_string());
    //     self
    // }

    // /// Associated a list of url params.
    // pub fn params<P, K, V>(mut self, pairs: P) -> Self
    // where
    //     P: IntoIterator,
    //     P::Item: Borrow<(K, V)>,
    //     K: AsRef<str>,
    //     V: ToString,
    // {
    //     for pair in pairs.into_iter() {
    //         let (key, value) = pair.borrow();
    //         self.params.insert(key.as_ref(), &value.to_string());
    //     }
    //     self
    // }

    /// Enable HTTP basic authentication.
    pub fn basic_auth(self, username: impl std::fmt::Display, password: Option<impl std::fmt::Display>) -> Self {
        let auth = match password {
            Some(password) => format!("{}:{}", username, password),
            None => format!("{}:", username),
        };
        let mut encoded = String::from("Basic ");
        base64::encode_config_buf(auth.as_bytes(), base64::STANDARD, &mut encoded);
        self.add_header(header::AUTHORIZATION, encoded, true)
    }

    /// Enable HTTP bearer authentication.
    pub fn bearer_auth(self, token: impl Into<String>) -> Self {
        self.add_header(header::AUTHORIZATION, format!("Bearer {}", token.into()), true)
    }

    /// Sets the body of this request.
    pub fn body(mut self, body: impl Into<Body>) -> Self {
        self.body = body.into();
        self
    }

    /// Sets the body of this request to be text.
    ///
    /// If the `Content-Type` header is unset, it will be set to `text/plain` and the charset to UTF-8.
    pub fn text(mut self, body: impl Into<String>) -> Self {
        self.headers
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("text/plain; charset=utf-8"));
        self.body(body.into())
    }

    /// Sets the body of this request to be bytes.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/octet-stream`.
    pub fn bytes(mut self, body: Vec<u8>) -> Self {
        self.headers
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/octet-stream"));
        self.body(body)
    }

    /// Sets the body of this request to be the JSON representation of the given object.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/json` and the charset to UTF-8.
    pub fn json<T: serde::Serialize>(mut self, value: &T) -> Self {
        self.headers
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json; charset=utf-8"));
        self.body(serde_json::to_vec(value).unwrap())
    }

    /// Sets the body of this request to be the JSON representation of the given string.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/json` and the charset to UTF-8.
    pub fn raw_json(mut self, value: impl Into<String>) -> Self {
        self.headers
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json; charset=utf-8"));
        self.body(value.into())
    }

    /// Sets the body of this request to be the URL-encoded representation of the given object.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/x-www-form-urlencoded`.
    pub fn form<T: serde::Serialize>(mut self, value: &T) -> Self {
        let body = serde_urlencoded::to_string(value).unwrap().into_bytes();
        self.headers
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/x-www-form-urlencoded"));
        self.body(body)
    }
    /// Sets the body of this request to be the URL-encoded representation of the given string.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/x-www-form-urlencoded`.
    pub fn raw_form(mut self, value: impl Into<String>) -> Self {
        self.headers
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/x-www-form-urlencoded"));
        self.body(value.into())
    }
    /// Modify a header for this response.
    ///
    /// When `overwrite` is set to `true`, If the header is already present, the value will be replaced.
    /// When `overwrite` is set to `false`, The new header is always appended to the request, even if the header already exists.
    pub fn add_header<N, V>(mut self, name: N, value: V, overwrite: bool) -> Self
    where
        N: IntoHeaderName,
        V: TryInto<HeaderValue>,
    {
        let value = value
            .try_into()
            .map_err(|_| Error::Other("invalid header value".into()))
            .unwrap();
        if overwrite {
            self.headers.insert(name, value);
        } else {
            self.headers.append(name, value);
        }
        self
    }

    /// Build final request.
    pub fn build(self) -> Request {
        self.build_hyper().into()
    }

    /// Build hyper request.
    pub fn build_hyper(self) -> hyper::Request<Body> {
        let Self {
            url,
            method,
            headers,
            body,
        } = self;
        let mut req = hyper::Request::builder().method(method).uri(url.to_string());
        (*req.headers_mut().unwrap()) = headers;
        req.body(body).unwrap()
    }

    /// Send request to target, such as [`Router`], [`Service`], [`Handler`].
    pub async fn send(self, target: impl SendTarget) -> Response {
        let mut response = target.call(self.build()).await;
        #[cfg(feature = "cookie")]
        {
            let values = response
                .cookies
                .delta()
                .filter_map(|c| c.encoded().to_string().parse().ok())
                .collect::<Vec<_>>();
            for hv in values {
                response.headers_mut().insert(header::SET_COOKIE, hv);
            }
        }
        response
    }
}
#[async_trait]
pub trait SendTarget {
    #[must_use = "future must be used"]
    async fn call(self, req: Request) -> Response;
}
#[async_trait]
impl SendTarget for &Service {
    async fn call(self, req: Request) -> Response {
        self.handle(req).await
    }
}
#[async_trait]
impl SendTarget for Router {
    async fn call(self, req: Request) -> Response {
        let router = Arc::new(self);
        SendTarget::call(router, req).await
    }
}
#[async_trait]
impl SendTarget for Arc<Router> {
    async fn call(self, req: Request) -> Response {
        let srv = Service::new(self);
        srv.handle(req).await
    }
}

#[async_trait]
impl<T> SendTarget for Arc<T>
where
    T: Handler + Send,
{
    async fn call(self, req: Request) -> Response {
        let mut req = req;
        let mut depot = Depot::new();
        #[cfg(not(feature = "cookie"))]
        let mut res = Response::new();
        #[cfg(feature = "cookie")]
        let mut res = Response::with_cookies(req.cookies.clone());
        let mut ctrl = FlowCtrl::new(vec![self.clone()]);
        self.handle(&mut req, &mut depot, &mut res, &mut ctrl).await;
        res
    }
}
#[async_trait]
impl<T> SendTarget for T
where
    T: Handler + Send,
{
    async fn call(self, req: Request) -> Response {
        let handler = Arc::new(self);
        SendTarget::call(handler, req).await
    }
}
