//! Iron's HTTP Request representation and associated methods.
use std::fmt::{self, Debug};
use std::net::SocketAddr;
use std::cell::{RefCell, Ref};
use url::{Url};
use std::borrow::Cow::Borrowed;
use double_checked_cell::DoubleCheckedCell;

use http;
use http::version::Version as HttpVersion;

use http::method::Method;
use futures::{Future, Stream};

#[cfg(test)]
use std::net::ToSocketAddrs;

use std::collections::HashMap;
use crate::http::headers::{self, HeaderMap};
use crate::http::{Body, Mime};
use crate::http::form::{self, FormData, Error as FormError};
use crate::error::Error;
use crate::{Protocol};
use cookie::{Cookie, CookieJar};

/// The `Request` given to all `Middleware`.
///
/// Stores all the properties of the client's request plus
/// an `TypeMap` for data communication between middleware.
pub struct Request {
    /// The requested URL.
    url: Url,

    /// The local address of the request.
    local_addr: Option<SocketAddr>,

    /// The request headers.
    headers: HeaderMap,

    /// The request body as a reader.
    body: RefCell<Option<Body>>,

    /// The request method.
    method: Method,

    cookies: CookieJar,

    // accept: Option<Vec<Mime>>,
    queries: DoubleCheckedCell<HashMap<String, String>>,
    form_data: DoubleCheckedCell<Result<FormData, FormError>>,
    body_data: DoubleCheckedCell<Result<Vec<u8>, Error>>,

    /// The version of the HTTP protocol used.
    version: HttpVersion,
}

impl Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Request {{")?;

        writeln!(f, "    url: {:?}", self.url)?;
        writeln!(f, "    method: {:?}", self.method.clone())?;
        writeln!(f, "    local_addr: {:?}", self.local_addr)?;

        write!(f, "}}")?;
        Ok(())
    }
}

impl Request {
    /// Create a request from an hyper::Request.
    ///
    /// This constructor consumes the hyper::Request.
    pub fn from_hyper(req: hyper::Request<Body>, local_addr: Option<SocketAddr>, protocol: &Protocol) -> Result<Request, String> {
        let (
            http::request::Parts {
                method,
                uri,
                version,
                headers,
                ..
            },
            body,
        ) = req.into_parts();

        let url = {
            let path = uri.path();

            let mut socket_ip = String::new();
            let (host, port) = if let Some(host) = uri.host() {
                (host, uri.port_part().map(|p| p.as_u16()))
            } else if let Some(host) = headers.get(headers::HOST).and_then(|h| h.to_str().ok()) {
                let mut parts = host.split(':');
                let hostname = parts.next().unwrap();
                let port = parts.next().and_then(|p| p.parse::<u16>().ok());
                (hostname, port)
            } else if version < HttpVersion::HTTP_11 {
                if let Some(local_addr) = local_addr {
                    match local_addr {
                        SocketAddr::V4(addr4) => socket_ip.push_str(&format!("{}", addr4.ip())),
                        SocketAddr::V6(addr6) => socket_ip.push_str(&format!("[{}]", addr6.ip())),
                    }
                    (socket_ip.as_ref(), Some(local_addr.port()))
                } else {
                    return Err("No fallback host specified".into());
                }
            } else {
                return Err("No host specified in request".into());
            };

            let url_string = if let Some(port) = port {
                format!("{}://{}:{}{}", protocol.name(), host, port, path)
            } else {
                format!("{}://{}{}", protocol.name(), host, path)
            };

            match Url::parse(&url_string) {
                Ok(url) => url,
                Err(e) => return Err(format!("Couldn't parse requested URL: {}", e)),
            }
        };

        // Set the request cookies, if they exist.
        let cookies = if let Some(header) = headers.get("Cookie") {
            let mut cookie_jar = CookieJar::new();
            if let Ok(header) = header.to_str() {
                for cookie_str in header.split(';').map(|s| s.trim()) {
                    if let Some(cookie) = Cookie::parse_encoded(cookie_str).map(|c| c.into_owned()).ok() {
                        cookie_jar.add_original(cookie);
                    }
                }
            }
            cookie_jar
        } else {
            CookieJar::new()
        };

        Ok(Request {
            queries: DoubleCheckedCell::new(),
            url,
            local_addr,
            headers,
            body: RefCell::new(Some(body)),
            method,
            cookies,
            // accept: None,
            form_data: DoubleCheckedCell::new(),
            body_data: DoubleCheckedCell::new(),
            version,
        })
    }

    #[inline(always)]
    pub fn url(&self) -> &Url {
        &self.url
    }

    #[inline(always)]
    pub fn method(&self) -> &Method {
        &self.method
    }

    #[inline(always)]
    pub fn version(&self) -> HttpVersion {
        self.version
    }

    #[inline(always)]
    pub fn body(&self) -> Ref<Option<Body>> {
        self.body.borrow()
    }

    // #[inline(always)]
    // pub fn body_mut(&mut self) -> Option<&mut Body> {
    //     self.body.borrow().as_mut()
    // }
    

    #[inline(always)]
    pub fn take_body(&mut self) -> Option<Body> {
        self.body.replace(None)
    }

    pub fn queries(&self) -> &HashMap<String, String>{
        self.queries.get_or_init(||self.url.query_pairs().into_owned().collect())
    }
    pub fn form_data(&self) -> &Result<FormData, FormError>{
        self.form_data.get_or_init(||{
            let bdata = self.body_data().as_ref();
            if let Err(_) = bdata {
                Err(FormError::Decoding(Borrowed("get body data error")))
            }else{
                let mut reader = bdata.unwrap().as_slice();
                form::read_form_data(&mut reader, &self.headers)
            }
        })
    }
    pub fn body_data(&self) -> &Result<Vec<u8>, Error> {
        self.body_data.get_or_init(||{
            let body = self.body.replace(None);
            body.ok_or(Error::General("empty body".to_owned()))
            .and_then(|body|body.concat2().wait().map_err(|_|Error::General("parse body error".to_owned())))
            .map(|body|body.to_vec()).map_err(|_|Error::General("parse body error".to_owned()))
        })
    }
    pub fn accept(&self) -> Vec<Mime> {
        let mut list: Vec<Mime> = vec![];
        if let Some(accept) = self.headers.get("accept").and_then(|h| h.to_str().ok()) {
            let parts: Vec<&str> = accept.split(',').collect();
            for part in parts {
                if let Ok(mt) = part.parse() {
                    list.push(mt);
                }
            }
        }
        list
    }
    pub fn frist_accept(&self) -> Option<Mime> {
        let mut accept = self.accept();
        if accept.len() > 0 {
            Some(accept.remove(0))
        }else{
            None
        }
    }
    pub fn content_type(&self) -> Option<Mime> {
        return if let Some(ctype) = self.headers.get("content-type").and_then(|h| h.to_str().ok()) {
            return ctype.parse().ok()
        } else{
            None
        }
    }
    
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }


    #[inline(always)]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

}

pub trait BodyReader: Send {
}