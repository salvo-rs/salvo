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

use hyper::body::Incoming as HyperBody;
use hyper::upgrade::OnUpgrade;
use once_cell::sync::OnceCell;
use percent_encoding::{utf8_percent_encode, CONTROLS};
use salvo_core::http::header::{HeaderMap, HeaderName, HeaderValue, CONNECTION, HOST, UPGRADE};
use salvo_core::http::uri::{Scheme, Uri};
use salvo_core::http::ReqBody;
use salvo_core::http::StatusCode;
use salvo_core::{async_trait, BoxedError, Depot, Error, FlowCtrl, Handler, Request, Response};
use salvo_rustls::{HttpsConnector, HttpsConnectorBuilder};
use salvo_utils::client::{connect::HttpConnector, legacy::Client};
use salvo_utils::rt::TokioExecutor;
use tokio::io::copy_bidirectional;

type HyperRequest = hyper::Request<ReqBody>;
type HyperResponse = hyper::Response<HyperBody>;

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

/// Proxy
pub struct Proxy<U> {
    upstreams: U,
    http_client: OnceCell<Client<HttpConnector, ReqBody>>,
    https_client: OnceCell<Client<HttpsConnector<HttpConnector>, ReqBody>>,
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
            http_client: OnceCell::new(),
            https_client: OnceCell::new(),
        }
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

    #[inline]
    fn build_proxied_request(&self, req: &mut Request) -> Result<HyperRequest, Error> {
        let upstream = self.upstreams.elect().map_err(Error::other)?;
        if upstream.is_empty() {
            tracing::error!("upstreams is empty");
            return Err(Error::other("upstreams is empty"));
        }

        let param = req.params().iter().find(|(key, _)| key.starts_with('*'));
        let mut rest = if let Some((_, rest)) = param {
            encode_url_path(rest)
        } else {
            "".into()
        };
        if let Some(query) = req.uri().query() {
            rest = format!("{}?{}", rest, query);
        }

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
        let is_https = proxied_request
            .uri()
            .scheme()
            .map(|s| s == &Scheme::HTTPS)
            .unwrap_or(false);
        let mut response = if is_https {
            let client = self.https_client.get_or_init(|| {
                let connector = HttpsConnectorBuilder::new()
                    .with_webpki_roots()
                    .https_or_http()
                    .enable_http1()
                    .enable_http2()
                    .build();
                Client::builder(TokioExecutor::new()).build::<_, ReqBody>(connector)
            });
            client.request(proxied_request).await.map_err(Error::other)?
        } else {
            let client = self
                .http_client
                .get_or_init(|| Client::builder(TokioExecutor::new()).build::<_, ReqBody>(HttpConnector::new()));
            client.request(proxied_request).await.map_err(Error::other)?
        };

        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            let response_upgrade_type = get_upgrade_type(response.headers());

            if request_upgrade_type.as_deref() == response_upgrade_type {
                let mut response_upgraded = response
                    .extensions_mut()
                    .remove::<OnUpgrade>()
                    .ok_or_else(|| Error::other("response does not have an upgrade extension"))?
                    .await?;
                if let Some(request_upgraded) = request_upgraded {
                    tokio::spawn(async move {
                        match request_upgraded.await {
                            Ok(mut request_upgraded) => {
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
        }
        Ok(response)
    }
}

#[async_trait]
impl<U> Handler for Proxy<U>
where
    U: Upstreams,
    U::Error: Into<BoxedError>,
{
    #[inline]
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        match self.build_proxied_request(req) {
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
                        res.body(body.into());
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

    //TODO: https://github.com/hyperium/http-body/issues/88
    // #[tokio::test]
    // async fn test_proxy() {
    //     let router = Router::new()
    //         .push(Router::with_path("rust/<**rest>").handle(Proxy::new(vec!["https://www.rust-lang.org"])));

    //     let content = TestClient::get("http://127.0.0.1:5801/rust/tools/install")
    //         .send(router)
    //         .await
    //         .take_string()
    //         .await
    //         .unwrap();
    //     assert!(content.contains("Install Rust"));
    // }
    #[test]
    fn test_others() {
        let mut handler = Proxy::new(["https://www.bing.com"]);
        assert_eq!(handler.upstreams().len(), 1);
        assert_eq!(handler.upstreams_mut().len(), 1);
    }
}
