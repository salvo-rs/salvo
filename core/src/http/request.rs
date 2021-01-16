use cookie::{Cookie, CookieJar};
use double_checked_cell_async::DoubleCheckedCell;
use http;
use http::header::{self, HeaderMap};
use http::method::Method;
use http::version::Version as HttpVersion;
use multimap::MultiMap;
use once_cell::sync::OnceCell;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::str::FromStr;
use url::Url;

use crate::http::errors::ReadError;
use crate::http::form::{self, FilePart, FormData};
use crate::http::header::{AsHeaderName, HeaderValue};
use crate::http::{Body, Mime};

/// The `Request` given to all `Middleware`.
///
/// Stores all the properties of the client's request plus
/// an `TypeMap` for data communication between middleware.
pub struct Request {
    /// The requested URL.
    url: Url,

    /// The request headers.
    headers: HeaderMap,

    /// The request body as a reader.
    body: Option<Body>,

    /// The request method.
    method: Method,

    cookies: CookieJar,

    pub(crate) params: HashMap<String, String>,

    // accept: Option<Vec<Mime>>,
    queries: OnceCell<MultiMap<String, String>>,
    form_data: DoubleCheckedCell<FormData>,
    payload: DoubleCheckedCell<Vec<u8>>,

    /// The version of the HTTP protocol used.
    version: HttpVersion,
}

impl Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Request {{")?;

        writeln!(f, "    url: {:?}", self.url)?;
        writeln!(f, "    method: {:?}", self.method.clone())?;

        write!(f, "}}")?;
        Ok(())
    }
}

impl Request {
    /// Create a request from an hyper::Request.
    ///
    /// This constructor consumes the hyper::Request.
    pub fn from_hyper(req: hyper::Request<Body>) -> Result<Request, String> {
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
            let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("");
            let url_string = if let (Some(scheme), Some(authority)) = (uri.scheme(), uri.authority()) {
                format!("{}://{}{}", scheme.as_str(), authority.as_str(), path_and_query)
            } else {
                return Err(format!("couldn't parse requested url: {}", uri.to_string()))
            };

            match Url::parse(&url_string) {
                Ok(url) => url,
                Err(e) => return Err(format!("couldn't parse requested url: {}", e)),
            }
        };

        // Set the request cookies, if they exist.
        let cookies = if let Some(header) = headers.get("Cookie") {
            let mut cookie_jar = CookieJar::new();
            if let Ok(header) = header.to_str() {
                for cookie_str in header.split(';').map(|s| s.trim()) {
                    if let Ok(cookie) = Cookie::parse_encoded(cookie_str).map(|c| c.into_owned()) {
                        cookie_jar.add_original(cookie);
                    }
                }
            }
            cookie_jar
        } else {
            CookieJar::new()
        };

        Ok(Request {
            queries: OnceCell::new(),
            url,
            headers,
            body: Some(body),
            method,
            cookies,
            // accept: None,
            params: HashMap::new(),
            form_data: DoubleCheckedCell::new(),
            payload: DoubleCheckedCell::new(),
            // multipart: OnceCell::new(),
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
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn content_type(&self) -> Option<Mime> {
        if let Some(ctype) = self.headers.get("content-type").and_then(|h| h.to_str().ok()) {
            ctype.parse().ok()
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }
    #[inline]
    pub fn get_cookie<T>(&self, name: T) -> Option<&Cookie<'static>>
    where
        T: AsRef<str>,
    {
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
    pub fn get_param<'a, F>(&self, key: &'a str) -> Option<F>
    where
        F: FromStr,
    {
        self.params.get(key).and_then(|v| v.parse::<F>().ok())
    }

    pub fn queries(&self) -> &MultiMap<String, String> {
        self.queries.get_or_init(|| self.url.query_pairs().into_owned().collect())
    }
    #[inline]
    pub fn get_query<'a, F>(&self, key: &'a str) -> Option<F>
    where
        F: FromStr,
    {
        self.queries().get(key).and_then(|v| v.parse::<F>().ok())
    }

    #[inline]
    pub async fn get_form<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.form_data()
            .await
            .as_ref()
            .ok()
            .and_then(|ps| ps.fields.get(key))
            .and_then(|v| v.parse::<F>().ok())
    }
    #[inline]
    pub async fn get_file(&mut self, key: &str) -> Option<&FilePart> {
        self.form_data().await.as_ref().ok().and_then(|ps| ps.files.get(key))
    }
    #[inline]
    pub async fn get_form_or_query<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.get_form(key.as_ref()).await.or_else(|| self.get_query(key))
    }
    #[inline]
    pub async fn get_query_or_form<F>(&mut self, key: &str) -> Option<F>
    where
        F: FromStr,
    {
        self.get_query(key.as_ref()).or(self.get_form(key).await)
    }
    pub async fn payload(&mut self) -> Result<&Vec<u8>, ReadError> {
        let ctype = self.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        if ctype == "application/x-www-form-urlencoded" || ctype.starts_with("multipart/form-data") {
            Err(ReadError::General(String::from("failed to read data1")))
        } else if ctype.starts_with("application/json") || ctype.starts_with("text/") {
            let body = self.body.take();
            self.payload
                .get_or_try_init(async {
                    match body {
                        Some(body) => read_body_bytes(body).await,
                        None => Err(ReadError::General(String::from("failed to read data2"))),
                    }
                })
                .await
        } else {
            Err(ReadError::General(String::from("failed to read data3")))
        }
    }

    pub async fn form_data(&mut self) -> Result<&FormData, ReadError> {
        let ctype = self.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        if ctype == "application/x-www-form-urlencoded" || ctype.starts_with("multipart/form-data") {
            let body = self.body.take();
            let headers = self.headers();
            self.form_data
                .get_or_try_init(async {
                    match body {
                        Some(body) => form::read_form_data(headers, body).await,
                        None => Err(ReadError::General("empty body".into())),
                    }
                })
                .await
        } else {
            Err(ReadError::General(String::from("failed to read data4")))
        }
    }

    #[inline]
    pub async fn read_from_json<T>(&mut self) -> Result<T, ReadError>
    where
        T: DeserializeOwned,
    {
        match self.payload().await {
            Ok(body) => Ok(serde_json::from_slice::<T>(&body)?),
            Err(_) => Err(ReadError::General("ddd".into())),
        }
    }
    #[inline]
    pub async fn read_from_form<T>(&mut self) -> Result<T, ReadError>
    where
        T: DeserializeOwned,
    {
        match self.form_data().await {
            Ok(form_data) => {
                let data = serde_json::to_value(&form_data.fields)?;
                Ok(serde_json::from_value::<T>(data)?)
            }
            Err(_) => Err(ReadError::General("ddd".into())),
        }
    }
    #[inline]
    pub async fn read<T>(&mut self) -> Result<T, ReadError>
    where
        T: DeserializeOwned,
    {
        let ctype = self.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        if ctype == "application/x-www-form-urlencoded" || ctype.starts_with("multipart/form-data") {
            self.read_from_form().await
        } else if ctype.starts_with("application/json") {
            self.read_from_json().await
        } else {
            Err(ReadError::General(String::from("failed to read data5")))
        }
    }
}

pub trait BodyReader: Send {}
// pub(crate) async fn read_body_cursor<B: HttpBody>(body: B) -> Result<Cursor<Vec<u8>>, ReadError> {
//     Ok(Cursor::new(read_body_bytes(body).await?))
// }
pub(crate) async fn read_body_bytes(body: Body) -> Result<Vec<u8>, ReadError> {
    hyper::body::to_bytes(body)
        .await
        .map_err(|_| ReadError::General("read body bytes error".into()))
        .map(|d| d.to_vec())
}
