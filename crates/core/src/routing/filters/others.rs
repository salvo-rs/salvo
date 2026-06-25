use std::fmt::{self, Debug, Formatter};

use crate::async_trait;
use crate::http::uri::Scheme;
use crate::http::{Method, Request};
use crate::routing::{Filter, FilterInfo, PathState};

/// Filter by request method
#[derive(Clone, PartialEq, Eq)]
pub struct MethodFilter(pub Method);
impl MethodFilter {
    /// Create a new `MethodFilter`.
    #[must_use]
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
    #[inline]
    fn info(&self) -> FilterInfo {
        FilterInfo::Method(self.0.clone())
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
    /// Fallback filter result returned when the request URI has no scheme.
    pub fallback: bool,
}
impl SchemeFilter {
    /// Create a new `SchemeFilter`.
    #[must_use]
    pub fn new(scheme: Scheme) -> Self {
        Self {
            scheme,
            fallback: false,
        }
    }
    /// Sets the fallback filter result returned when the request URI has no scheme.
    #[must_use]
    pub fn fallback(mut self, fallback: bool) -> Self {
        self.fallback = fallback;
        self
    }

    /// Sets the fallback filter result returned when the request URI has no scheme.
    #[deprecated(since = "0.94.0", note = "use `SchemeFilter::fallback` instead")]
    #[must_use]
    pub fn lack(self, lack: bool) -> Self {
        self.fallback(lack)
    }
}

#[async_trait]
impl Filter for SchemeFilter {
    #[inline]
    async fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.uri()
            .scheme()
            .map(|s| s == &self.scheme)
            .unwrap_or(self.fallback)
    }
    #[inline]
    fn info(&self) -> FilterInfo {
        FilterInfo::Scheme(self.scheme.clone())
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
    /// Fallback filter result returned when the request URI has no host.
    pub fallback: bool,
}
impl HostFilter {
    /// Create a new `HostFilter`.
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            fallback: false,
        }
    }
    /// Sets the fallback filter result returned when the request URI has no host.
    #[must_use]
    pub fn fallback(mut self, fallback: bool) -> Self {
        self.fallback = fallback;
        self
    }

    /// Sets the fallback filter result returned when the request URI has no host.
    #[deprecated(since = "0.94.0", note = "use `HostFilter::fallback` instead")]
    #[must_use]
    pub fn lack(self, lack: bool) -> Self {
        self.fallback(lack)
    }
}

/// Split an authority into its host and optional port.
///
/// Handles bracketed IPv6 literals such as `[::1]` and `[::1]:8080` correctly; a
/// naive `rsplit_once(':')` would split inside the address.
fn split_host_port(authority: &str) -> (&str, Option<&str>) {
    if authority.starts_with('[') {
        // IPv6 literal: the host ends at the closing bracket.
        if let Some(close) = authority.find(']') {
            let host = &authority[..=close];
            let port = authority[close + 1..]
                .strip_prefix(':')
                .filter(|p| !p.is_empty());
            return (host, port);
        }
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    }
}

#[async_trait]
impl Filter for HostFilter {
    #[inline]
    async fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        // On HTTP/1 without the `fix-http1-request-uri` feature, the URI has no authority,
        // so fall back to the `Host` header. See https://github.com/hyperium/hyper/issues/1310
        #[cfg(feature = "fix-http1-request-uri")]
        let host = req.uri().authority().map(|a| a.as_str());
        #[cfg(not(feature = "fix-http1-request-uri"))]
        let host = req.uri().authority().map(|a| a.as_str()).or_else(|| {
            req.headers()
                .get(crate::http::header::HOST)
                .and_then(|h| h.to_str().ok())
        });
        host.map(|h| split_host_port(h).0 == self.host)
            .unwrap_or(self.fallback)
    }
    #[inline]
    fn info(&self) -> FilterInfo {
        FilterInfo::Host(self.host.clone())
    }
}
impl Debug for HostFilter {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "host:{:?}", self.host)
    }
}

/// Filter by request URI port.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PortFilter {
    /// Port to filter.
    pub port: u16,
    /// Fallback filter result returned when the request URI has no port.
    pub fallback: bool,
}

impl PortFilter {
    /// Create a new `PortFilter`.
    #[must_use]
    pub fn new(port: u16) -> Self {
        Self {
            port,
            fallback: false,
        }
    }
    /// Sets the fallback filter result returned when the request URI has no port.
    #[must_use]
    pub fn fallback(mut self, fallback: bool) -> Self {
        self.fallback = fallback;
        self
    }

    /// Sets the fallback filter result returned when the request URI has no port.
    #[deprecated(since = "0.94.0", note = "use `PortFilter::fallback` instead")]
    #[must_use]
    pub fn lack(self, lack: bool) -> Self {
        self.fallback(lack)
    }
}

#[async_trait]
impl Filter for PortFilter {
    #[inline]
    async fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        // On HTTP/1 without the `fix-http1-request-uri` feature, the URI has no authority,
        // so fall back to the `Host` header. See https://github.com/hyperium/hyper/issues/1310
        #[cfg(feature = "fix-http1-request-uri")]
        let host = req.uri().authority().map(|a| a.as_str());
        #[cfg(not(feature = "fix-http1-request-uri"))]
        let host = req.uri().authority().map(|a| a.as_str()).or_else(|| {
            req.headers()
                .get(crate::http::header::HOST)
                .and_then(|h| h.to_str().ok())
        });
        host.and_then(|h| split_host_port(h).1)
            .and_then(|p| p.parse::<u16>().ok())
            .map(|p| p == self.port)
            .unwrap_or(self.fallback)
    }
    #[inline]
    fn info(&self) -> FilterInfo {
        FilterInfo::Port(self.port)
    }
}
impl Debug for PortFilter {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "port:{:?}", self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_host_port_handles_ipv4_and_ipv6() {
        assert_eq!(split_host_port("example.com"), ("example.com", None));
        assert_eq!(
            split_host_port("example.com:8080"),
            ("example.com", Some("8080"))
        );
        // IPv6 literals must not be split inside the address.
        assert_eq!(split_host_port("[::1]"), ("[::1]", None));
        assert_eq!(split_host_port("[::1]:8080"), ("[::1]", Some("8080")));
        assert_eq!(
            split_host_port("[2001:db8::1]:443"),
            ("[2001:db8::1]", Some("443"))
        );
    }

    #[tokio::test]
    async fn fallback_sets_scheme_filter_result_when_scheme_is_absent() {
        let mut req = Request::new();
        let mut state = PathState::new(req.uri().path());

        assert!(
            SchemeFilter::new(Scheme::HTTPS)
                .fallback(true)
                .filter(&mut req, &mut state)
                .await
        );
    }

    #[tokio::test]
    async fn fallback_sets_host_filter_result_when_host_is_absent() {
        let mut req = Request::new();
        let mut state = PathState::new(req.uri().path());

        assert!(
            HostFilter::new("example.com")
                .fallback(true)
                .filter(&mut req, &mut state)
                .await
        );
    }

    #[tokio::test]
    async fn fallback_sets_port_filter_result_when_port_is_absent() {
        let mut req = Request::new();
        let mut state = PathState::new(req.uri().path());

        assert!(
            PortFilter::new(443)
                .fallback(true)
                .filter(&mut req, &mut state)
                .await
        );
    }
}
