//! Proxy.
use std::borrow::Cow;
use std::convert::TryFrom;
use std::sync::Mutex;

use hyper::upgrade::OnUpgrade;
use hyper::{Client, Uri};
use hyper_rustls::HttpsConnectorBuilder;
use salvo_core::async_trait;
use salvo_core::http::header::{HeaderMap, HeaderName, HeaderValue, CONNECTION};
use salvo_core::http::uri::Scheme;
use salvo_core::prelude::*;
use salvo_core::{Error, Result};
use tokio::io::copy_bidirectional;

type HyperRequest = hyper::Request<hyper::body::Body>;
type HyperResponse = hyper::Response<hyper::body::Body>;

static CONNECTION_HEADER: HeaderName = HeaderName::from_static("connection");
static UPGRADE_HEADER: HeaderName = HeaderName::from_static("upgrade");

/// Proxy
pub struct Proxy {
    upstreams: Vec<String>,
    counter: Mutex<usize>,
}

impl Proxy {
    /// Create new `Proxy` with upstreams list.
    pub fn new(upstreams: Vec<String>) -> Self {
        if upstreams.is_empty() {
            panic!("proxy upstreams is empty");
        }
        Proxy {
            upstreams,
            counter: Mutex::new(0),
        }
    }

    /// Get upstreams list.
    #[inline]
    pub fn upstreams(&self) -> &Vec<String> {
        &self.upstreams
    }
    /// Get upstreams mutable list.
    #[inline]
    pub fn upstreams_mut(&mut self) -> &mut Vec<String> {
        &mut self.upstreams
    }
    /// Set upstreams list and return Self.
    #[inline]
    pub fn with_upstreams(mut self, upstreams: Vec<String>) -> Self {
        self.upstreams = upstreams;
        self
    }
}
impl Proxy {
    fn build_proxied_request(&self, req: &mut Request) -> Result<HyperRequest> {
        let upstream = if self.upstreams.len() > 1 {
            let mut counter = self.counter.lock().unwrap();
            let upstream = self.upstreams.get(*counter);
            *counter = (*counter + 1) % self.upstreams.len();
            upstream
        } else if !self.upstreams.is_empty() {
            self.upstreams.get(0)
        } else {
            tracing::error!("upstreams is empty");
            return Err(Error::other("upstreams is empty"));
        }
        .map(|s| &**s)
        .unwrap_or_default();
        if upstream.is_empty() {
            tracing::error!("upstreams is empty");
            return Err(Error::other("upstreams is empty"));
        }

        let param = req.params().iter().find(|(key, _)| key.starts_with('*'));
        let mut rest: Cow<'_, str> = if let Some((_, rest)) = param {
            rest.into()
        } else {
            "".into()
        };
        if let Some(query) = req.uri().query() {
            rest = format!("{}?{}", rest, query).into();
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
            if key.as_str() != "host" {
                build = build.header(key, value);
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
        build.body(req.take_body().unwrap_or_default()).map_err(Error::other)
    }

    async fn call_proxied_server(
        &self,
        proxied_request: HyperRequest,
        request_upgraded: Option<OnUpgrade>,
    ) -> Result<HyperResponse> {
        let request_upgrade_type = get_upgrade_type(proxied_request.headers());
        let is_https = proxied_request
            .uri()
            .scheme()
            .map(|s| s == &Scheme::HTTPS)
            .unwrap_or(false);
        let mut response = if is_https {
            let client = Client::builder().build::<_, hyper::Body>(
                HttpsConnectorBuilder::new()
                    .with_webpki_roots()
                    .https_or_http()
                    .enable_http1()
                    .enable_http2()
                    .build(),
            );
            client.request(proxied_request).await?
        } else {
            let client = Client::new();
            client.request(proxied_request).await?
        };

        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            let response_upgrade_type = get_upgrade_type(response.headers());

            if request_upgrade_type == response_upgrade_type {
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
impl Handler for Proxy {
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
                        println!("==============={:?}  =={:?}  {:#?}", req.uri(), status, headers);
                        res.set_status_code(status);
                        res.set_headers(headers);
                        res.set_body(body.into());
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, uri = ?req.uri(), "get response data failed");
                        res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
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
fn get_upgrade_type(headers: &HeaderMap) -> Option<String> {
    println!("===xxxxx {:?}", headers.get(&CONNECTION_HEADER));
    if headers
        .get(&CONNECTION_HEADER)
        .map(|value| value.to_str().unwrap().split(',').any(|e| e.trim() == UPGRADE_HEADER))
        .unwrap_or(false)
    {
        if let Some(upgrade_value) = headers.get(&UPGRADE_HEADER) {
            tracing::debug!(
                "Found upgrade header with value: {}",
                upgrade_value.to_str().unwrap().to_owned()
            );

            return Some(upgrade_value.to_str().unwrap().to_owned());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[tokio::test]
    #[should_panic]
    async fn test_proxy_painc() {
        Proxy::new(vec![]);
    }

    #[tokio::test]
    async fn test_proxy() {
        let router = Router::new()
            .push(Router::with_path("baidu/<**rest>").handle(Proxy::new(vec!["https://www.baidu.com".into()])));

        let content = TestClient::get("http://127.0.0.1:7979/baidu?wd=rust")
            .send(router)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("baidu"));
    }
    #[test]
    fn test_others() {
        let mut handler = Proxy::new(vec!["https://www.baidu.com".into()]);
        assert_eq!(handler.upstreams().len(), 1);
        assert_eq!(handler.upstreams_mut().len(), 1);
        let handler = handler.with_upstreams(vec!["https://www.baidu.com".into(), "https://www.baidu.com".into()]);
        assert_eq!(handler.upstreams().len(), 2);
    }
}
