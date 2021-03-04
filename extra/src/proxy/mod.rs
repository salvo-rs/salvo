//! ProxyHandler.
use async_trait::async_trait;
use hyper::header::CONNECTION;
use hyper::Client;
use salvo_core::prelude::*;
use salvo_core::Result;
use std::sync::{Arc, Mutex};
use std::fmt;

#[derive(Debug)]
struct MsgError {
    msg: String
}

impl MsgError {
    fn new(msg: impl Into<String>) -> MsgError {
        MsgError{msg: msg.into()}
    }
}

impl fmt::Display for MsgError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"{}",self.msg)
    }
}

impl std::error::Error for MsgError {}

pub struct ProxyHandler {
    pub upstreams: Vec<String>,
    counter: Arc<Mutex<usize>>,
}

impl ProxyHandler {
    pub fn new(upstreams: Vec<String>) -> Self {
        if upstreams.is_empty() {
            panic!("proxy upstreams is empty");
        }
        ProxyHandler {
            upstreams,
            counter: Arc::new(Mutex::new(0)),
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
        }.map(|s|&**s).unwrap_or_default();
        if upstream.is_empty() {
            tracing::error!("upstreams is empty");
            return Err(salvo_core::Error::new(MsgError::new("upstreams is empty")));
        }

        let param = req.params().iter().find(|(key, _)| key.starts_with('*'));
        let rest = if let Some((_, rest)) = param { rest } else { "" }.strip_prefix('/').unwrap_or("");
        let forward_url = if let Some(query) = req.uri().query() {
            if rest.is_empty() {
                format!("{}?{}", upstream, query)
            } else {
                format!("{}/{}?{}", upstream.strip_suffix('/').unwrap_or(""), rest, query)
            }
        } else {
            if rest.is_empty() {
                upstream.into()
            } else {
                format!("{}{}", upstream.strip_suffix('/').unwrap_or(""), rest)
            }
        };
        let mut build = hyper::Request::builder().method(req.method()).version(req.version()).uri(forward_url);
        for (key, value) in req.headers() {
            build = build.header(key, value);
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
        build.body(req.take_body().unwrap_or_default()).map_err(|e|salvo_core::Error::new(e))
    }
}

#[async_trait]
impl Handler for ProxyHandler {
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        match self.build_proxied_request(req) {
            Ok(proxied_request) => {
                let client = Client::new();
                match client.request(proxied_request).await {
                    Ok(response) => {
                        let (
                            http::response::Parts {
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
