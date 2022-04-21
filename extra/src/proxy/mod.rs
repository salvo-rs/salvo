//! ProxyHandler.
#![allow(clippy::mutex_atomic)]
use std::convert::TryFrom;
use std::sync::Mutex;

use hyper::{Client, Uri};
use hyper_rustls::HttpsConnectorBuilder;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use salvo_core::async_trait;
use salvo_core::http::header::{HeaderName, HeaderValue, CONNECTION};
use salvo_core::http::uri::Scheme;
use salvo_core::prelude::*;
use salvo_core::{Error, Result};

/// ProxyHandler
pub struct ProxyHandler {
    upstreams: Vec<String>,
    counter: Mutex<usize>,
}

impl ProxyHandler {
    /// Create new `ProxyHandler` with upstreams list.
    pub fn new(upstreams: Vec<String>) -> Self {
        if upstreams.is_empty() {
            panic!("proxy upstreams is empty");
        }
        ProxyHandler {
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
    /// set upstreams list and return Self.
    #[inline]
    pub fn with_upstreams(mut self, upstreams: Vec<String>) -> Self {
        self.upstreams = upstreams;
        self
    }
}
impl ProxyHandler {
    fn build_proxied_request(&self, req: &mut Request) -> Result<hyper::Request<hyper::body::Body>> {
        req.headers_mut().remove(CONNECTION);
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
        let rest = if let Some((_, rest)) = param { rest } else { "" }.trim_start_matches('/');
        let forward_url = if let Some(query) = req.uri().query() {
            if rest.is_empty() {
                format!("{}?{}", upstream, query)
            } else {
                format!("{}/{}?{}", upstream.trim_end_matches('/'), encode_url_path(rest), query)
            }
        } else if rest.is_empty() {
            upstream.into()
        } else {
            format!("{}/{}", upstream.trim_end_matches('/'), encode_url_path(rest))
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
}

#[async_trait]
impl Handler for ProxyHandler {
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        match self.build_proxied_request(req) {
            Ok(proxied_request) => {
                let response = if proxied_request
                    .uri()
                    .scheme()
                    .map(|s| s == &Scheme::HTTPS)
                    .unwrap_or(false)
                {
                    let client = Client::builder().build::<_, hyper::Body>(
                        HttpsConnectorBuilder::new()
                            .with_webpki_roots()
                            .https_or_http()
                            .enable_http1()
                            .enable_http2()
                            .build(),
                    );
                    client.request(proxied_request).await
                } else {
                    let client = Client::new();
                    client.request(proxied_request).await
                };
                match response {
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
                        res.set_status_code(status);
                        res.set_headers(headers);
                        res.set_body(Some(body.into()));
                    }
                    Err(e) => {
                        tracing::error!("error: {}", e);
                        res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                };
                res.headers_mut().remove(CONNECTION);
            }
            Err(e) => {
                tracing::error!("error when build proxied request: {}", e);
            }
        }
        if ctrl.has_next() {
            tracing::error!("all handlers after ProxyHandler will skipped");
            ctrl.skip_reset();
        }
    }
}

fn encode_url_path(path: &str) -> String {
    path.split('/')
        .map(|s| utf8_percent_encode(s, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use salvo_core::hyper;
    use salvo_core::prelude::*;

    use super::*;

    #[tokio::test]
    #[should_panic]
    async fn test_proxy_painc() {
        ProxyHandler::new(vec![]);
    }

    #[tokio::test]
    async fn test_proxy() {
        let router = Router::new()
            .push(Router::with_path("baidu/<**rest>").handle(ProxyHandler::new(vec!["https://www.baidu.com".into()])));
        let service = Service::new(router);

        let req: Request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/baidu?wd=rust")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let content = service.handle(req).await.take_text().await.unwrap();
        assert!(content.contains("baidu"));
    }
    #[test]
    fn test_others() {
        let mut handler = ProxyHandler::new(vec!["https://www.baidu.com".into()]);
        assert_eq!(handler.upstreams().len(), 1);
        assert_eq!(handler.upstreams_mut().len(), 1);
        let handler = handler.with_upstreams(vec!["https://www.baidu.com".into(), "https://www.baidu.com".into()]);
        assert_eq!(handler.upstreams().len(), 2);
    }
}
