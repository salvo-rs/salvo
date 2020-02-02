use std::fmt::{self, Debug};
use std::net::SocketAddr;
use std::cell::{RefCell, Ref};
use std::str::FromStr;
use std::collections::HashMap;
use url::Url;
use multimap::MultiMap;
use double_checked_cell_async::DoubleCheckedCell as ADoubleCheckedCell;
use double_checked_cell::DoubleCheckedCell;
use serde::de::DeserializeOwned;
use http;
use http::version::Version as HttpVersion;
use http::method::Method;
use http::header::{self, HeaderMap};
use cookie::{Cookie, CookieJar};
use std::io::Cursor;
use hyper::body::HttpBody;
use tokio::runtime::Runtime;

#[cfg(test)]
use std::net::ToSocketAddrs;

use crate::Protocol;
use crate::http::{Body, Mime};
use crate::http::form::{self, FormData, FilePart};
use crate::http::multipart::Multipart;
use crate::http::header::{AsHeaderName, HeaderValue};
use crate::http::errors::ReadError;
use atomic_refcell::{AtomicRefCell, AtomicRef};

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
    body: Option<Body>,

    /// The request method.
    method: Method,

    cookies: CookieJar,

    pub(crate) params: HashMap<String, String>,

    // accept: Option<Vec<Mime>>,
    queries: DoubleCheckedCell<MultiMap<String, String>>,
    form_data: ADoubleCheckedCell<Result<FormData, ReadError>>,
    // multipart: DoubleCheckedCell<Result<Multipart, ReadError>>,
    payload: ADoubleCheckedCell<Result<Vec<u8>, ReadError>>,

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
            body: Some(body),
            method,
            cookies,
            // accept: None,
            params: HashMap::new(),
            form_data: ADoubleCheckedCell::new(),
            payload: ADoubleCheckedCell::new(),
            // multipart: DoubleCheckedCell::new(),
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
    pub fn body(&self) -> Option<&Body> {
        self.body.as_ref()
    }

    // #[inline(always)]
    // pub fn body_mut(&mut self) -> Option<&mut Body> {
    //     self.body.borrow().as_mut()
    // }
    

    #[inline(always)]
    pub fn take_body(&mut self) -> Option<Body> {
        self.body.take()
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
        self.queries.get_or_init(||{self.url.query_pairs().into_owned().collect()})
    }
    #[inline]
    pub fn get_query<'a, F>(&self, key: &'a str) -> Option<F> where F: FromStr {
        self.queries().get(key).and_then(|v|v.parse::<F>().ok())
    }
    
    #[inline]
    pub async fn get_form<'a, F>(&mut self, key: &'a str) -> Option<F> where F: FromStr {
        self.form_data().await.as_ref().ok().and_then(|ps|ps.fields.get(key)).and_then(|v|v.parse::<F>().ok())
    }
    #[inline]
    pub async fn get_file<'a>(&mut self, key: &'a str) -> Option<&FilePart> {
        self.form_data().await.as_ref().ok().and_then(|ps|ps.files.get(key))
    }
    #[inline]
    pub async fn get_form_or_query<'a, F>(&mut self, key: &'a str) -> Option<F> where F: FromStr {
        self.get_form(key.as_ref()).await.or(self.get_query(key))
    }
    #[inline]
    pub async fn get_query_or_form<'a, F>(&mut self, key: &'a str) -> Option<F> where F: FromStr {
        self.get_query(key.as_ref()).or(self.get_form(key).await)
    }
    pub async fn payload(&mut self) -> &Result<Vec<u8>, ReadError> {
        let ctype = self.headers().get(header::CONTENT_TYPE).map(|t|t.clone());
        match ctype {
            Some(ctype) if ctype == "application/x-www-form-urlencoded" || ctype.to_str().unwrap_or("").starts_with("multipart/form-data") => {
                self.payload.get_or_init(async {Err(ReadError::General(String::from("failed to read data1")))}).await
            },
            Some(ctype) if ctype == "application/json" || ctype.to_str().unwrap_or("").starts_with("text/") => {
                let body = self.body.take();
                self.payload.get_or_init(async {
                    match body {
                        Some(body) => {
                            read_body_bytes(body).await
                        },
                        None => Err(ReadError::General(String::from("failed to read data2"))),
                    }
                }).await
            },
            _=> self.payload.get_or_init(async {Err(ReadError::General(String::from("failed to read data3")))}).await,
        }
    }
    
    // pub async fn multipart(&mut self) -> &Result<Multipart, ReadError> {
    //     let ctype = self.headers().get(header::CONTENT_TYPE).map(|t|t.clone());
    //     let body = self.body.take();
    //     self.multipart.get_or_init(|| {
    //         match ctype {
    //             Some(ctype) if ctype.to_str().unwrap_or("").starts_with("multipart/form-data") => {
    //                 Multipart::try_from_body_headers(body, self.headers())
    //             },
    //             _=> {
    //                 self.body = body;
    //                 Err(ReadError::General(String::from("failed to read data3")))
    //             },
    //         }
    //     })
    // }
    
    pub async fn form_data(&mut self) -> &Result<FormData, ReadError>{
        let ctype = self.headers().get(header::CONTENT_TYPE).map(|t|t.clone());
        match ctype {
            Some(ctype) if ctype == "application/x-www-form-urlencoded" || ctype.to_str().unwrap_or("").starts_with("multipart/form-data") => { 
                let body = self.body.take();
                let headers = self.headers();
                self.form_data.get_or_init(async {
                    match body {
                        Some(body) => {
                            form::read_form_data(headers, body).await
                        },
                        None => {
                            // self.body = body;
                            Err(ReadError::General("empty body".into()))
                        },
                    }
                }).await
            },
            _=> {
                self.form_data.get_or_init(async {Err(ReadError::General(String::from("failed to read data4")))}).await
            },
        }
    }
    
    #[inline]
    pub async fn read_from_json<T>(&mut self) -> Result<T, ReadError> where T: DeserializeOwned {
        match self.payload().await {
            Ok(body) => Ok(serde_json::from_slice::<T>(&body)?),
            Err(_) => Err(ReadError::General("ddd".into())),
        }
    }
    #[inline]
    pub async fn read_from_form<T>(&mut self) -> Result<T, ReadError> where T: DeserializeOwned {
        match self.form_data().await {
            Ok(form_data) => {
                let data = serde_json::to_value(&form_data.fields)?;
                Ok(serde_json::from_value::<T>(data)?)
            },
            Err(_) => Err(ReadError::General("ddd".into())),
        }
    }
    #[inline]
    pub async fn read<T>(&mut self) -> Result<T, ReadError> where T: DeserializeOwned  {
        match self.headers().get(header::CONTENT_TYPE) {
            Some(ctype) if ctype == "application/x-www-form-urlencoded" || ctype.to_str().unwrap_or("").starts_with("multipart/form-data") => self.read_from_form().await,
            Some(ctype) if ctype == "application/json" => self.read_from_json().await,
            _=> Err(ReadError::General(String::from("failed to read data5")))
        }
    }
}

pub trait BodyReader: Send {
}
// pub(crate) async fn read_body_cursor<B: HttpBody>(body: B) -> Result<Cursor<Vec<u8>>, ReadError> {
//     Ok(Cursor::new(read_body_bytes(body).await?))
// }
pub(crate) async fn read_body_bytes(body: Body) -> Result<Vec<u8>, ReadError> {
    hyper::body::to_bytes(body).await.map_err(|_|ReadError::General("ddd".into())).map(|d|d.to_vec())
}