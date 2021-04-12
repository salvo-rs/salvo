//! ProxyHandler.
#![allow(clippy::mutex_atomic)]
use std::convert::TryFrom;
use std::fmt;
use std::sync::Mutex;

use async_trait::async_trait;
use hyper::{Client, Uri};
use hyper_tls::HttpsConnector;
use salvo_core::http::header::{HeaderName, HeaderValue, CONNECTION};
use salvo_core::http::uri::Scheme;
use salvo_core::prelude::*;
use salvo_core::{Error, Result};

#[derive(Debug)]
struct MsgError {
    msg: String,
}

impl MsgError {
    fn new(msg: impl Into<String>) -> MsgError {
        MsgError { msg: msg.into() }
    }
}

impl fmt::Display for MsgError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for MsgError {}

pub struct ProxyHandler {
    pub upstreams: Vec<String>,
    counter: Mutex<usize>,
}

impl ProxyHandler {
    pub fn new(upstreams: Vec<String>) -> Self {
        if upstreams.is_empty() {
            panic!("proxy upstreams is empty");
        }
        ProxyHandler {
            upstreams,
            counter: Mutex::new(0),
        }
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
            return Err(salvo_core::Error::new(MsgError::new("upstreams is empty")));
        }
        .map(|s| &**s)
        .unwrap_or_default();
        if upstream.is_empty() {
            tracing::error!("upstreams is empty");
            return Err(salvo_core::Error::new(MsgError::new("upstreams is empty")));
        }

        let param = req.params().iter().find(|(key, _)| key.starts_with('*'));
        let rest = if let Some((_, rest)) = param { rest } else { "" }.trim_start_matches('/');
        let forward_url = if let Some(query) = req.uri().query() {
            if rest.is_empty() {
                format!("{}?{}", upstream, query)
            } else {
                format!("{}/{}?{}", upstream.trim_end_matches('/'), rest, query)
            }
        } else if rest.is_empty() {
            upstream.into()
        } else {
            format!("{}/{}", upstream.trim_end_matches('/'), rest)
        };
        let forward_url: Uri = TryFrom::try_from(forward_url).map_err(Error::new)?;
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
        build
            .body(req.take_body().unwrap_or_default())
            .map_err(salvo_core::Error::new)
    }
}

#[async_trait]
impl Handler for ProxyHandler {
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        match self.build_proxied_request(req) {
            Ok(proxied_request) => {
                let response = if proxied_request
                    .uri()
                    .scheme()
                    .map(|s| s == &Scheme::HTTPS)
                    .unwrap_or(false)
                {
                    let client = Client::builder().build::<_, hyper::Body>(HttpsConnector::new());
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
    }
}
