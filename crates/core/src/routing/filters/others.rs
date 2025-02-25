use std::fmt::{self, Debug, Formatter};

use crate::async_trait;
use crate::http::uri::Scheme;
use crate::http::{Method, Request};
use crate::routing::{Filter, PathState};

/// Filter by request method
#[derive(Clone, PartialEq, Eq)]
pub struct MethodFilter(pub Method);
impl MethodFilter {
    /// Create a new `MethodFilter`.
    pub fn new(method: Method) -> Self {
        Self(method)
    }
}

#[async_trait]
impl Filter for MethodFilter {
    #[inline]
    async fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.method() == self.0
    }
}
impl Debug for MethodFilter {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "method:{:?}", self.0)
    }
}

///  Filter by request URI scheme.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SchemeFilter {
    /// Scheme to filter.
    pub scheme: Scheme,
    /// When scheme is lack in request uri, use this value.
    pub lack: bool,
}
impl SchemeFilter {
    /// Create a new `SchemeFilter`.
    pub fn new(scheme: Scheme) -> Self {
        Self {
            scheme,
            lack: false,
        }
    }
    /// Set lack value and return `Self`.
    pub fn lack(mut self, lack: bool) -> Self {
        self.lack = lack;
        self
    }
}

#[async_trait]
impl Filter for SchemeFilter {
    #[inline]
    async fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.uri()
            .scheme()
            .map(|s| s == &self.scheme)
            .unwrap_or(self.lack)
    }
}
impl Debug for SchemeFilter {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "scheme:{:?}", self.scheme)
    }
}

/// Filter by request uri host.
#[derive(Clone, PartialEq, Eq)]
pub struct HostFilter {
    /// Host to filter.
    pub host: String,
    /// When host is lack in request uri, use this value.
    pub lack: bool,
}
impl HostFilter {
    /// Create a new `HostFilter`.
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            lack: false,
        }
    }
    /// Set lack value and return `Self`.
    pub fn lack(mut self, lack: bool) -> Self {
        self.lack = lack;
        self
    }
}

#[async_trait]
impl Filter for HostFilter {
    #[inline]
    async fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        // Http1, if `fix-http1-request-uri` feature is disabled, host is lack. so use header host instead.
        // https://github.com/hyperium/hyper/issues/1310
        #[cfg(feature = "fix-http1-request-uri")]
        let host = req.uri().authority().map(|a| a.as_str());
        #[cfg(not(feature = "fix-http1-request-uri"))]
        let host = req.uri().authority().map(|a| a.as_str()).or_else(|| {
            req.headers()
                .get(crate::http::header::HOST)
                .and_then(|h| h.to_str().ok())
        });
        host.map(|h| {
            if h.contains(':') {
                h.rsplit_once(':')
                    .expect("rsplit_once by ':' should not returns `None`")
                    .0
            } else {
                h
            }
        })
        .map(|h| h == self.host)
        .unwrap_or(self.lack)
    }
}
impl Debug for HostFilter {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "host:{:?}", self.host)
    }
}

/// Filter by request uri host.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PortFilter {
    /// Port to filter.
    pub port: u16,
    /// When port is lack in request uri, use this value.
    pub lack: bool,
}

impl PortFilter {
    /// Create a new `PortFilter`.
    pub fn new(port: u16) -> Self {
        Self { port, lack: false }
    }
    /// Set lack value and return `Self`.
    pub fn lack(mut self, lack: bool) -> Self {
        self.lack = lack;
        self
    }
}

#[async_trait]
impl Filter for PortFilter {
    #[inline]
    async fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        // Http1, if `fix-http1-request-uri` feature is disabled, port is lack. so use header host instead.
        // https://github.com/hyperium/hyper/issues/1310
        #[cfg(feature = "fix-http1-request-uri")]
        let host = req.uri().authority().map(|a| a.as_str());
        #[cfg(not(feature = "fix-http1-request-uri"))]
        let host = req.uri().authority().map(|a| a.as_str()).or_else(|| {
            req.headers()
                .get(crate::http::header::HOST)
                .and_then(|h| h.to_str().ok())
        });
        host.map(|h| {
            if h.contains(':') {
                h.rsplit_once(':')
                    .expect("rsplit_once by ':' should not returns `None`")
                    .1
            } else {
                h
            }
        })
        .and_then(|p| p.parse::<u16>().ok())
        .map(|p| p == self.port)
        .unwrap_or(self.lack)
    }
}
impl Debug for PortFilter {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "port:{:?}", self.port)
    }
}
