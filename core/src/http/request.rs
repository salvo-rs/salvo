use std::fmt::{self, Debug};
use std::net::SocketAddr;
use std::cell::{RefCell, Ref};
use std::borrow::Cow::Borrowed;
use std::str::FromStr;
use std::collections::HashMap;
use url::Url;
use multimap::MultiMap;
use double_checked_cell::DoubleCheckedCell;
use serde::de::DeserializeOwned;
use http;
use http::version::Version as HttpVersion;
use http::method::Method;
use http::header::{self, HeaderMap};
use cookie::{Cookie, CookieJar};
use futures::stream::TryStreamExt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(test)]
use std::net::ToSocketAddrs;

use crate::Protocol;
use crate::http::{Body, Mime};
use crate::http::form::FilePart;
use crate::http::form::{self, FormData};
use crate::http::header::{AsHeaderName, HeaderValue};
use crate::http::errors::ReadError;

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

    pub(crate) params: HashMap<String, String>,

    // accept: Option<Vec<Mime>>,
    queries: DoubleCheckedCell<MultiMap<String, String>>,
    form_data: DoubleCheckedCell<Result<FormData, ReadError>>,
    payload: DoubleCheckedCell<Result<Vec<u8>, ReadError>>,

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
            let path_and_query = uri.path_and_query().map(|pq|pq.as_str()).unwrap_or("");

            let mut socket_ip = String::new();
            let (host, port) = if let Some(host) = uri.host() {
                (host, uri.port().map(|p| p.as_u16()))
            } else if let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) {
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
                format!("{}://{}:{}{}", protocol.name(), host, port, path_and_query)
            } else {
                format!("{}://{}{}", protocol.name(), host, path_and_query)
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
            params: HashMap::new(),
            form_data: DoubleCheckedCell::new(),
            payload: DoubleCheckedCell::new(),
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
    
    #[inline(always)]
    pub fn frist_accept(&self) -> Option<Mime> {
        let mut accept = self.accept();
        if !accept.is_empty() {
            Some(accept.remove(0))
        }else{
            None
        }
    }

    #[inline(always)]
    pub fn content_type(&self) -> Option<Mime> {
        if let Some(ctype) = self.headers.get("content-type").and_then(|h| h.to_str().ok()) {
            ctype.parse().ok()
        } else{
            None
        }
    }
    
    #[inline(always)]
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }
    #[inline]
    pub fn get_cookie<T>(&self, name:T) -> Option<&Cookie<'static>> where T: AsRef<str> {
         self.cookies.get(name.as_ref())
    }


    #[inline(always)]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    #[inline]
    pub fn get_header<K: AsHeaderName>(&self, key: K) -> Option<&HeaderValue> {
        self.headers.get(key)
    }
    #[inline(always)]
    pub fn params(&self) -> &HashMap<String, String> {
        &self.params
    }
    
    #[inline]
    pub fn get_param<'a, F>(&self, key: &'a str) -> Option<F> where F: FromStr {
        self.params.get(key).and_then(|v|v.parse::<F>().ok())
    }

    pub fn queries(&self) -> &MultiMap<String, String>{
        self.queries.get_or_init(||self.url.query_pairs().into_owned().collect())
    }
    #[inline]
    pub fn get_query<'a, F>(&self, key: &'a str) -> Option<F> where F: FromStr {
        self.queries().get(key).and_then(|v|v.parse::<F>().ok())
    }
    
    #[inline]
    pub fn get_form<'a, F>(&self, key: &'a str) -> Option<F> where F: FromStr {
        self.form_data().as_ref().ok().and_then(|ps|ps.fields.get(key)).and_then(|v|v.parse::<F>().ok())
    }
    #[inline]
    pub fn get_file<'a>(&self, key: &'a str) -> Option<&FilePart> {
        self.form_data().as_ref().ok().and_then(|ps|ps.files.get(key))
    }
    #[inline]
    pub fn get_form_or_query<'a, F>(&self, key: &'a str) -> Option<F> where F: FromStr {
        self.get_form(key.as_ref()).or(self.get_query(key))
    }
    #[inline]
    pub fn get_query_or_form<'a, F>(&self, key: &'a str) -> Option<F> where F: FromStr {
        self.get_query(key.as_ref()).or(self.get_form(key))
    }

    pub fn payload(&self) -> &Result<Vec<u8>, ReadError> {
        self.payload.get_or_init(||{
            match self.headers().get(header::CONTENT_TYPE) {
                Some(ctype) if ctype == "application/x-www-form-urlencoded" || ctype == "multipart/form-data" => {
                    Err(ReadError::General(String::from("failed to read data")))
                },
                Some(ctype) if ctype == "application/json" || ctype.to_str().unwrap_or("").starts_with("text/") => {
                    match self.take_body() {
                        Some(body) => {
                            Ok(hyper::body::to_bytes(body).wait()?)
                        },
                        None => Err(ReadError::General(String::from("failed to read data"))),
                    }
                },
                _=> Err(ReadError::General(String::from("failed to read data"))),
            }
        })
    }
    
    pub fn form_data(&self) -> &Result<FormData, ReadError>{
        self.form_data.get_or_init(||{
            match self.headers().get(header::CONTENT_TYPE) {
                Some(ctype) if ctype == "application/x-www-form-urlencoded" || ctype == "multipart/form-data" => {
                    match self.take_body() {
                        Some(body) => form::read_form_data(&mut body, &self.headers),
                        None => Err(ReadError::General("empty body".into())),
                    }
                },
                _=> Err(ReadError::General(String::from("failed to read data"))),
            }
        })
    }
    
    #[inline]
    pub fn read_from_json<T>(&self) -> Result<T, ReadError> where T: DeserializeOwned {
        self.payload().and_then(|body|Ok(serde_json::from_slice::<T>(&body)?))
    }
    #[inline]
    pub fn read_from_form<T>(&self) -> Result<T, ReadError> where T: DeserializeOwned {
        self.form_data().and_then(|form_data|{
            let data = serde_json::to_value(&form_data.fields)?;
            Ok(serde_json::from_value::<T>(data)?)
        })
    }
    #[inline]
    pub fn read<T>(&self) -> Result<T, ReadError> where T: DeserializeOwned  {
        match self.headers().get(header::CONTENT_TYPE) {
            Some(ctype) if ctype == "application/x-www-form-urlencoded" || ctype == "multipart/form-data" => self.read_from_form(),
            Some(ctype) if ctype == "application/json" => self.read_from_json(),
            _=> Err(ReadError::General(String::from("failed to read data")))
        }
    }
}

pub trait BodyReader: Send {
}

