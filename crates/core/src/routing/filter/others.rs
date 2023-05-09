use crate::http::uri::Scheme;
use crate::http::{Method, Request};
use crate::routing::{Filter, PathState};

/// Filter by request method
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodFilter(pub Method);
impl MethodFilter {
    /// Create a new `MethodFilter`.
    pub fn new(method: Method) -> Self {
        Self(method)
    }
}

impl Filter for MethodFilter {
    #[inline]
    fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.method() == self.0
    }
}

/// Filter by request uri scheme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemeFilter {
    /// Scheme to filter.
    pub scheme: Scheme,
    /// When scheme is lack in request uri, use this value.
    pub lack: bool,
}
impl SchemeFilter {
    /// Create a new `SchemeFilter`.
    pub fn new(scheme: Scheme) -> Self {
        Self { scheme, lack: false }
    }
    /// Set lack value and return `Self`.
    pub fn lack(mut self, lack: bool) -> Self {
        self.lack = lack;
        self
    }
}
impl Filter for SchemeFilter {
    #[inline]
    fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.uri().scheme().map(|s| s == &self.scheme).unwrap_or(self.lack)
    }
}

/// Filter by request uri host.
#[derive(Clone, Debug, PartialEq, Eq)]
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
impl Filter for HostFilter {
    #[inline]
    fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.uri().host().map(|h| h == self.host).unwrap_or(self.lack)
    }
}

/// Filter by request uri host.
#[derive(Clone, Debug, PartialEq, Eq)]
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
impl Filter for PortFilter {
    #[inline]
    fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.uri().port_u16().map(|p| p == self.port).unwrap_or(self.lack)
    }
}
