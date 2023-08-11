//! Proxy middleware.
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::future_not_send)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::convert::{Infallible, TryFrom};

use futures_util::TryStreamExt;
use hyper::upgrade::OnUpgrade;
use percent_encoding::{utf8_percent_encode, CONTROLS};
use reqwest::Client;
use salvo_core::http::header::{HeaderMap, HeaderName, HeaderValue, CONNECTION, HOST, UPGRADE};
use salvo_core::http::uri::Uri;
use salvo_core::http::{ReqBody, ResBody, StatusCode};
use salvo_core::rt::TokioIo;
use salvo_core::{async_trait, BoxedError, Depot, Error, FlowCtrl, Handler, Request, Response};
use tokio::io::copy_bidirectional;

type HyperRequest = hyper::Request<ReqBody>;
type HyperResponse = hyper::Response<ResBody>;

/// Encode url path. This can be used when build your custom url path getter.
#[inline]
pub(crate) fn encode_url_path(path: &str) -> String {
    path.split('/')
        .map(|s| utf8_percent_encode(s, CONTROLS).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Upstreams trait.
pub trait Upstreams: Send + Sync + 'static {
    /// Error type.
    type Error;
    /// Elect a upstream to process current request.
    fn elect(&self) -> Result<&str, Self::Error>;
}
impl Upstreams for &'static str {
    type Error = Infallible;

    fn elect(&self) -> Result<&str, Self::Error> {
        Ok(*self)
    }
}
impl Upstreams for String {
    type Error = Infallible;
    fn elect(&self) -> Result<&str, Self::Error> {
        Ok(self.as_str())
    }
}

impl<const N: usize> Upstreams for [&'static str; N] {
    type Error = Error;
    fn elect(&self) -> Result<&str, Self::Error> {
        if self.is_empty() {
            return Err(Error::other("upstreams is empty"));
        }
        let index = fastrand::usize(..self.len());
        Ok(self[index])
    }
}

impl<T> Upstreams for Vec<T>
where
    T: AsRef<str> + Send + Sync + 'static,
{
    type Error = Error;
    fn elect(&self) -> Result<&str, Self::Error> {
        if self.is_empty() {
            return Err(Error::other("upstreams is empty"));
        }
        let index = fastrand::usize(..self.len());
        Ok(self[index].as_ref())
    }
}

/// Url part getter. You can use this to get the proxied url path or query.
pub type UrlPartGetter = Box<dyn Fn(&Request, &Depot) -> Option<String> + Send + Sync + 'static>;

/// Default url path getter. This getter will get the url path from request wildcard param, like `<*rest>`, `<**rest>`.
pub fn default_url_path_getter(req: &Request, _depot: &Depot) -> Option<String> {
    let param = req.params().iter().find(|(key, _)| key.starts_with('*'));
    if let Some((_, rest)) = param {
        Some(encode_url_path(rest))
    } else {
        None
    }
}
/// Default url query getter. This getter just return the query string from request uri.
pub fn default_url_query_getter(req: &Request, _depot: &Depot) -> Option<String> {
    req.uri().query().map(Into::into)
}

/// Proxy
#[non_exhaustive]

pub struct Proxy<U> {
    /// Upstreams list.
    pub upstreams: U,
    /// [`Client`] for proxy.
    pub client: Client,
    /// Url path getter.
    pub url_path_getter: UrlPartGetter,
    /// Url query getter.
    pub url_query_getter: UrlPartGetter,
}

impl<U> Proxy<U>
where
    U: Upstreams,
    U::Error: Into<BoxedError>,
{
    /// Create new `Proxy` with upstreams list.
    pub fn new(upstreams: U) -> Self {
        Proxy {
            upstreams,
            client: Client::new(),
            url_path_getter: Box::new(default_url_path_getter),
            url_query_getter: Box::new(default_url_query_getter),
        }
    }
    /// Create new `Proxy` with upstreams list and [`Client`].
    pub fn with_client(upstreams: U, client: Client) -> Self {
        Proxy {
            upstreams,
            client,
            url_path_getter: Box::new(default_url_path_getter),
            url_query_getter: Box::new(default_url_query_getter),
        }
    }

    /// Set url path getter.
    #[inline]
    pub fn url_path_getter<G>(mut self, url_path_getter: G) -> Self
    where
        G: Fn(&Request, &Depot) -> Option<String> + Send + Sync + 'static,
    {
        self.url_path_getter = Box::new(url_path_getter);
        self
    }

    /// Set url query getter.
    #[inline]
    pub fn url_query_getter<G>(mut self, url_query_getter: G) -> Self
    where
        G: Fn(&Request, &Depot) -> Option<String> + Send + Sync + 'static,
    {
        self.url_query_getter = Box::new(url_query_getter);
        self
    }

    /// Get upstreams list.
    #[inline]
    pub fn upstreams(&self) -> &U {
        &self.upstreams
    }
    /// Get upstreams mutable list.
    #[inline]
    pub fn upstreams_mut(&mut self) -> &mut U {
        &mut self.upstreams
    }

    /// Get client reference.
    #[inline]
    pub fn client(&self) -> &Client {
        &self.client
    }
    /// Get client mutable reference.
    #[inline]
    pub fn client_mut(&mut self) -> &mut Client {
        &mut self.client
    }

    #[inline]
    fn build_proxied_request(&self, req: &mut Request, depot: &Depot) -> Result<HyperRequest, Error> {
        let upstream = self.upstreams.elect().map_err(Error::other)?;
        if upstream.is_empty() {
            tracing::error!("upstreams is empty");
            return Err(Error::other("upstreams is empty"));
        }

        let path = encode_url_path(&(self.url_path_getter)(req, depot).unwrap_or_default());
        let query = (self.url_query_getter)(req, depot);
        let rest = if let Some(query) = query {
            if query.starts_with('?') {
                format!("{}{}", path, query)
            } else {
                format!("{}?{}", path, query)
            }
        } else {
            path
        };
        let forward_url = if upstream.ends_with('/') && rest.starts_with('/') {
            format!("{}{}", upstream.trim_end_matches('/'), rest)
        } else if upstream.ends_with('/') || rest.starts_with('/') {
            format!("{}{}", upstream, rest)
        } else {
            format!("{}/{}", upstream, rest)
        };
        let forward_url: Uri = TryFrom::try_from(forward_url).map_err(Error::other)?;
        let mut build = hyper::Request::builder().method(req.method()).uri(&forward_url);
        for (key, value) in req.headers() {
            if key != HOST {
                build = build.header(key, value);
            } else {
                build = build.header(HOST, forward_url.host().unwrap());
            }
        }
        if let Some(host) = forward_url.host().and_then(|host| HeaderValue::from_str(host).ok()) {
            build = build.header(HeaderName::from_static("host"), host);
        }
        // let x_forwarded_for_header_name = "x-forwarded-for";
        // // Add forwarding information in the headers
        // match request.headers_mut().entry(x_forwarded_for_header_name) {
        //     Ok(header_entry) => {
        //         match header_entry {
        //             hyper::header::Entry::Vacant(entry) => {
        //                 let addr = format!("{}", client_ip);
        //                 entry.insert(addr.parse().unwrap());
        //             },
        //             hyper::header::Entry::Occupied(mut entry) => {
        //                 let addr = format!("{}, {}", entry.get().to_str().unwrap(), client_ip);
        //                 entry.insert(addr.parse().unwrap());
        //             }
        //         }
        //     }
        //     // shouldn't happen...
        //     Err(_) => panic!("Invalid header name: {}", x_forwarded_for_header_name),
        // }
        build.body(req.take_body()).map_err(Error::other)
    }

    #[inline]
    async fn call_proxied_server(
        &self,
        proxied_request: HyperRequest,
        request_upgraded: Option<OnUpgrade>,
    ) -> Result<HyperResponse, Error> {
        let request_upgrade_type = get_upgrade_type(proxied_request.headers()).map(|s| s.to_owned());

        let proxied_request = proxied_request.map(reqwest::Body::wrap_stream);
        let response = self
            .client
            .execute(proxied_request.try_into().map_err(Error::other)?)
            .await
            .map_err(Error::other)?;

        let res_headers = response.headers().clone();
        let hyper_response = hyper::Response::builder()
            .status(response.status())
            .version(response.version());

        let mut hyper_response = if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            let response_upgrade_type = get_upgrade_type(response.headers());

            if request_upgrade_type.as_deref() == response_upgrade_type {
                let mut response_upgraded = response
                    .upgrade()
                    .await
                    .map_err(|e| Error::other(format!("response does not have an upgrade extension. {}", e)))?;
                if let Some(request_upgraded) = request_upgraded {
                    tokio::spawn(async move {
                        match request_upgraded.await {
                            Ok(request_upgraded) => {
                                let mut request_upgraded = TokioIo::new(request_upgraded);
                                if let Err(e) = copy_bidirectional(&mut response_upgraded, &mut request_upgraded).await
                                {
                                    tracing::error!(error = ?e, "coping between upgraded connections failed");
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = ?e, "upgrade request failed");
                            }
                        }
                    });
                } else {
                    return Err(Error::other("request does not have an upgrade extension"));
                }
            } else {
                return Err(Error::other("upgrade type mismatch"));
            }
            hyper_response.body(ResBody::None).map_err(Error::other)?
        } else {
            hyper_response
                .body(ResBody::Stream(Box::pin(response.bytes_stream().map_err(|e| e.into()))))
                .map_err(Error::other)?
        };
        *hyper_response.headers_mut() = res_headers;
        Ok(hyper_response)
    }
}

#[async_trait]
impl<U> Handler for Proxy<U>
where
    U: Upstreams,
    U::Error: Into<BoxedError>,
{
    #[inline]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        match self.build_proxied_request(req, depot) {
            Ok(proxied_request) => {
                match self
                    .call_proxied_server(proxied_request, req.extensions_mut().remove())
                    .await
                {
                    Ok(response) => {
                        let (
                            salvo_core::http::response::Parts {
                                status,
                                // version,
                                headers,
                                // extensions,
                                ..
                            },
                            body,
                        ) = response.into_parts();
                        res.status_code(status);
                        res.set_headers(headers);
                        res.body(body);
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, uri = ?req.uri(), "get response data failed");
                        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                };
            }
            Err(e) => {
                tracing::error!(error = ?e, "build proxied request failed");
            }
        }
        if ctrl.has_next() {
            tracing::error!("all handlers after proxy will skipped");
            ctrl.skip_rest();
        }
    }
}
#[inline]
fn get_upgrade_type(headers: &HeaderMap) -> Option<&str> {
    if headers
        .get(&CONNECTION)
        .map(|value| value.to_str().unwrap().split(',').any(|e| e.trim() == UPGRADE))
        .unwrap_or(false)
    {
        if let Some(upgrade_value) = headers.get(&UPGRADE) {
            tracing::debug!("Found upgrade header with value: {:?}", upgrade_value.to_str());
            return upgrade_value.to_str().ok();
        }
    }

    None
}

// Unit tests for Proxy
#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::*;

    use super::*;

    #[test]
    fn test_encode_url_path() {
        let path = "/test/path";
        let encoded_path = encode_url_path(path);
        assert_eq!(encoded_path, "/test/path");
    }

    #[test]
    fn test_upstreams_elect() {
        let upstreams = vec!["https://www.example.com", "https://www.example2.com"];
        let proxy = Proxy::new(upstreams.clone());
        let elected_upstream = proxy.upstreams().elect().unwrap();
        assert!(upstreams.contains(&elected_upstream));
    }

    #[test]
    fn test_get_upgrade_type() {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION, HeaderValue::from_static("upgrade"));
        headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
        let upgrade_type = get_upgrade_type(&headers);
        assert_eq!(upgrade_type, Some("websocket"));
    }

    #[tokio::test]
    async fn test_proxy() {
        let router = Router::new()
            .push(Router::with_path("rust/<**rest>").handle(Proxy::new(vec!["https://www.rust-lang.org"])));

        let content = TestClient::get("http://127.0.0.1:5801/rust/tools/install")
            .send(router)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Install Rust"));
    }
    #[test]
    fn test_others() {
        let mut handler = Proxy::new(["https://www.bing.com"]);
        assert_eq!(handler.upstreams().len(), 1);
        assert_eq!(handler.upstreams_mut().len(), 1);
    }
}
