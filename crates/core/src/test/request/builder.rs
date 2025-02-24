use std::borrow::Borrow;
use std::str;
use std::sync::Arc;

use base64::engine::{general_purpose, Engine};
use http::header::{self, HeaderMap, HeaderValue, IntoHeaderName};
use http::uri::Scheme;
use url::Url;

use crate::http::body::ReqBody;
use crate::http::Method;
use crate::routing::{FlowCtrl, Router};
use crate::{Depot, Error, Handler, Request, Response, Service};

/// The main way of building [`Request`].
///
/// You can create a `RequestBuilder` using the `new` or `try_new` method, but the recommended way
/// or use one of the simpler constructors available in the [`TestClient`](crate::test::TestClient) struct,
/// such as `get`, `post`, etc.
#[derive(Debug)]
pub struct RequestBuilder {
    url: Url,
    method: Method,
    headers: HeaderMap,
    // params: HashMap<String, String>,
    body: ReqBody,
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
        let url = Url::parse(url.as_ref()).expect("invalid url");
        Self {
            url,
            method,
            headers: HeaderMap::new(),
            // params: HeaderMap::new(),
            body: ReqBody::None,
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
            Some(password) => format!("{username}:{password}"),
            None => format!("{username}:"),
        };
        let encoded = format!("Basic {}", general_purpose::STANDARD.encode(auth.as_bytes()));
        self.add_header(header::AUTHORIZATION, encoded, true)
    }

    /// Enable HTTP bearer authentication.
    pub fn bearer_auth(self, token: impl Into<String>) -> Self {
        self.add_header(header::AUTHORIZATION, format!("Bearer {}", token.into()), true)
    }

    /// Sets the body of this request.
    pub fn body(mut self, body: impl Into<ReqBody>) -> Self {
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
        self.body(serde_json::to_vec(value).expect("Failed to serialize json."))
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
        let body = serde_urlencoded::to_string(value)
            .expect("`serde_urlencoded::to_string` returns error")
            .into_bytes();
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
            .expect("invalid header value");
        if overwrite {
            self.headers.insert(name, value);
        } else {
            self.headers.append(name, value);
        }
        self
    }

    /// Build final request.
    pub fn build(self) -> Request {
        let req = self.build_hyper();
        let scheme = req.uri().scheme().cloned().unwrap_or(Scheme::HTTP);
        Request::from_hyper(req, scheme)
    }

    /// Build hyper request.
    pub fn build_hyper(self) -> hyper::Request<ReqBody> {
        let Self {
            url,
            method,
            headers,
            body,
        } = self;
        let mut req = hyper::Request::builder().method(method).uri(url.to_string());
        (*req.headers_mut().expect("`headers_mut` returns `None`")) = headers;
        req.body(body).expect("invalid request body")
    }

    /// Send request to target, such as [`Router`], [`Service`], [`Handler`].
    pub async fn send(self, target: impl SendTarget + Send) -> Response {
        #[cfg(feature = "cookie")]
        {
            let mut response = target.call(self.build()).await;
            let values = response
                .cookies
                .delta()
                .filter_map(|c| c.encoded().to_string().parse().ok())
                .collect::<Vec<_>>();
            for hv in values {
                response.headers_mut().insert(header::SET_COOKIE, hv);
            }
            response
        }
        #[cfg(not(feature = "cookie"))]
        target.call(self.build()).await
    }
}

/// Trait for sending request to target, such as [`Router`], [`Service`], [`Handler`] for test usage.
pub trait SendTarget {
    /// Send request to target, such as [`Router`], [`Service`], [`Handler`].
    #[must_use = "future must be used"]
    fn call(self, req: Request) -> impl Future<Output = Response> + Send;
}
impl SendTarget for &Service {
    async fn call(self, req: Request) -> Response {
        self.handle(req).await
    }
}
impl SendTarget for Router {
    async fn call(self, req: Request) -> Response {
        let router = Arc::new(self);
        SendTarget::call(router, req).await
    }
}
impl SendTarget for Arc<Router> {
    async fn call(self, req: Request) -> Response {
        let srv = Service::new(self);
        srv.handle(req).await
    }
}
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
impl<T> SendTarget for T
where
    T: Handler + Send,
{
    async fn call(self, req: Request) -> Response {
        let handler = Arc::new(self);
        SendTarget::call(handler, req).await
    }
}
