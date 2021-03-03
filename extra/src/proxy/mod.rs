//! ProxyHandler.
use async_trait::async_trait;
use hyper::header::CONNECTION;
use hyper::request::Parts;
use salvo_core::prelude::*;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct ProxyHandler {
    pub upstreams: Vec<String>,
    pub request_modifier: Option<fn(&mut Request, &mut Depot)>,
    pub response_modifier: Option<fn(&mut Response, &mut Depot)>,
    counter: Arc<Mutex<usize>>,
}

impl ProxyHandler {
    pub fn new(upstreams: Vec<String>) -> Self {
        if upstreams.is_empty() {
            panic!("proxy upstreams is empty");
        }
        ProxyHandler {
            upstreams,
            request_modifier: None,
            response_modifier: None,
            counter: Arc::new(Mutex::new(0)),
        }
    }
}

#[async_trait]
impl Handler for ProxyHandler {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        req.headers_mut().remove(CONNECTION);
        if let Some(request_modifier) = self.request_modifier {
            request_modifier(req, depot);
        }
        let upstream = if self.upstreams.len() > 1 {
            let mut counter = self.counter.lock().unwrap();
            let upstream = self.upstreams.get(*counter);
            *counter = (*counter + 1) % self.upstreams.len();
            upstream
        } else if !self.upstreams.is_empty() {
            self.upstreams.get(0)
        } else {
            tracing::error!("upstreams is empty");
            return;
        };
        if let Some(upstream) = upstream {
            let param = req.params().iter().find(|(key, _)| key.starts_with('*'));
            let rest = if let Some((_, rest)) = param { rest } else { "".into() }.strip_prefix('/').unwrap_or("");
            let forward_url = if let Some(query) = req.uri().query {
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
            let (parts, body) = req.into_parts();
            let mut proxied_request = Request::from_parts(parts, body);

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
            let client = Client::new();
	let proxied_response = client.request(proxied_request).then(|response| {
		let proxied_response = match response {
            Ok(response) => create_proxied_response(response),
            Err(e) => {
                tracing::error!("error: {}", e);
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap()
            },
        };

        }.await?;
        res.headers_mut().remove(CONNECTION);
        if let Some(response_modifier) = self.response_modifier {
            response_modifier(res, depot);
        }
    }
}
