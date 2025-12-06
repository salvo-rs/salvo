//! Provide HTTP proxy capabilities for the Salvo web framework.
//!
//! This crate allows you to easily forward requests to upstream servers,
//! supporting both HTTP and HTTPS protocols. It's useful for creating API gateways,
//! load balancers, and reverse proxies.
//!
//! # Example
//!
//! In this example, requests to different hosts are proxied to different upstream servers:
//! - Requests to <http://127.0.0.1:8698/> are proxied to <https://www.rust-lang.org>
//! - Requests to <http://localhost:8698/> are proxied to <https://crates.io>
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_proxy::Proxy;
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new()
//!         .push(
//!             Router::new()
//!                 .host("127.0.0.1")
//!                 .path("{**rest}")
//!                 .goal(Proxy::use_hyper_client("https://www.rust-lang.org")),
//!         )
//!         .push(
//!             Router::new()
//!                 .host("localhost")
//!                 .path("{**rest}")
//!                 .goal(Proxy::use_hyper_client("https://crates.io")),
//!         );
//!
//!     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::convert::Infallible;
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};

use hyper::upgrade::OnUpgrade;
use percent_encoding::{CONTROLS, utf8_percent_encode};
use salvo_core::conn::SocketAddr;
use salvo_core::http::header::{CONNECTION, HOST, HeaderMap, HeaderName, HeaderValue, UPGRADE};
use salvo_core::http::uri::Uri;
use salvo_core::http::{ReqBody, ResBody, StatusCode};
use salvo_core::{BoxedError, Depot, Error, FlowCtrl, Handler, Request, Response, async_trait};

#[cfg(test)]
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[cfg(not(test))]
use local_ip_address::{local_ip, local_ipv6};

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "hyper-client"]
    mod hyper_client;
    pub use hyper_client::*;
}
cfg_feature! {
    #![feature = "reqwest-client"]
    mod reqwest_client;
    pub use reqwest_client::*;
}

cfg_feature! {
    #![feature = "unix-sock-client"]
    #[cfg(unix)]
    mod unix_sock_client;
    #[cfg(unix)]
    pub use unix_sock_client::*;
}

type HyperRequest = hyper::Request<ReqBody>;
type HyperResponse = hyper::Response<ResBody>;

const X_FORWARDER_FOR_HEADER_NAME: &str = "x-forwarded-for";

/// Encode url path. This can be used when build your custom url path getter.
#[inline]
pub(crate) fn encode_url_path(path: &str) -> String {
    path.split('/')
        .map(|s| utf8_percent_encode(s, CONTROLS).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Client trait for implementing different HTTP clients for proxying.
///
/// Implement this trait to create custom proxy clients with different
/// backends or configurations.
pub trait Client: Send + Sync + 'static {
    /// Error type returned by the client.
    type Error: StdError + Send + Sync + 'static;

    /// Execute a request through the proxy client.
    fn execute(
        &self,
        req: HyperRequest,
        upgraded: Option<OnUpgrade>,
    ) -> impl Future<Output = Result<HyperResponse, Self::Error>> + Send;
}

/// Upstreams trait for selecting target servers.
///
/// Implement this trait to customize how target servers are selected
/// for proxying requests. This can be used to implement load balancing,
/// failover, or other server selection strategies.
pub trait Upstreams: Send + Sync + 'static {
    /// Error type returned when selecting a server fails.
    type Error: StdError + Send + Sync + 'static;

    /// Elect a server to handle the current request.
    fn elect(
        &self,
        req: &Request,
        depot: &Depot,
    ) -> impl Future<Output = Result<&str, Self::Error>> + Send;
}
impl Upstreams for &'static str {
    type Error = Infallible;

    async fn elect(&self, _: &Request, _: &Depot) -> Result<&str, Self::Error> {
        Ok(*self)
    }
}
impl Upstreams for String {
    type Error = Infallible;
    async fn elect(&self, _: &Request, _: &Depot) -> Result<&str, Self::Error> {
        Ok(self.as_str())
    }
}

impl<const N: usize> Upstreams for [&'static str; N] {
    type Error = Error;
    async fn elect(&self, _: &Request, _: &Depot) -> Result<&str, Self::Error> {
        if self.is_empty() {
            return Err(Error::other("upstreams is empty"));
        }
        let index = fastrand::usize(..self.len());
        Ok(self[index])
    }
}

impl<T> Upstreams for Vec<T>
where
    T: AsRef<str> + Send + Sync + 'static,
{
    type Error = Error;
    async fn elect(&self, _: &Request, _: &Depot) -> Result<&str, Self::Error> {
        if self.is_empty() {
            return Err(Error::other("upstreams is empty"));
        }
        let index = fastrand::usize(..self.len());
        Ok(self[index].as_ref())
    }
}

/// Url part getter. You can use this to get the proxied url path or query.
pub type UrlPartGetter = Box<dyn Fn(&Request, &Depot) -> Option<String> + Send + Sync + 'static>;

/// Host header getter. You can use this to get the host header for the proxied request.
pub type HostHeaderGetter =
    Box<dyn Fn(&Uri, &Request, &Depot) -> Option<String> + Send + Sync + 'static>;

/// Default url path getter.
///
/// This getter will get the last param as the rest url path from request.
/// In most case you should use wildcard param, like `{**rest}`, `{*+rest}`.
pub fn default_url_path_getter(req: &Request, _depot: &Depot) -> Option<String> {
    req.params().tail().map(encode_url_path)
}
/// Default url query getter. This getter just return the query string from request uri.
pub fn default_url_query_getter(req: &Request, _depot: &Depot) -> Option<String> {
    req.uri().query().map(Into::into)
}

/// Default host header getter. This getter will get the host header from request uri
pub fn default_host_header_getter(
    forward_uri: &Uri,
    _req: &Request,
    _depot: &Depot,
) -> Option<String> {
    if let Some(host) = forward_uri.host() {
        return Some(String::from(host));
    }

    None
}

/// RFC2616 complieant host header getter. This getter will get the host header from request uri, and add port if
/// it's not default port. Falls back to default upon any forward URI parse error.
pub fn rfc2616_host_header_getter(
    forward_uri: &Uri,
    req: &Request,
    _depot: &Depot,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::with_capacity(2);

    if let Some(host) = forward_uri.host() {
        parts.push(host.to_owned());

        if let Some(scheme) = forward_uri.scheme_str()
            && let Some(port) = forward_uri.port_u16()
            && (scheme == "http" && port != 80 || scheme == "https" && port != 443)
        {
            parts.push(port.to_string());
        }
    }

    if parts.is_empty() {
        default_host_header_getter(forward_uri, req, _depot)
    } else {
        Some(parts.join(":"))
    }
}

/// Preserve original host header getter. Propagates the original request host header to the proxied request.
pub fn preserve_original_host_header_getter(
    forward_uri: &Uri,
    req: &Request,
    _depot: &Depot,
) -> Option<String> {
    if let Some(host_header) = req.headers().get(HOST)
        && let Ok(host) = String::from_utf8(host_header.as_bytes().to_vec())
    {
        return Some(host);
    }

    default_host_header_getter(forward_uri, req, _depot)
}

/// Handler that can proxy request to other server.
#[non_exhaustive]
pub struct Proxy<U, C>
where
    U: Upstreams,
    C: Client,
{
    /// Upstreams list.
    pub upstreams: U,
    /// [`Client`] for proxy.
    pub client: C,
    /// Url path getter.
    pub url_path_getter: UrlPartGetter,
    /// Url query getter.
    pub url_query_getter: UrlPartGetter,
    /// Host header getter
    pub host_header_getter: HostHeaderGetter,
    /// Flag to enable x-forwarded-for header.
    pub client_ip_forwarding_enabled: bool,
}

impl<U, C> Debug for Proxy<U, C>
where
    U: Upstreams,
    C: Client,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Proxy").finish()
    }
}

impl<U, C> Proxy<U, C>
where
    U: Upstreams,
    U::Error: Into<BoxedError>,
    C: Client,
{
    /// Create new `Proxy` with upstreams list.
    #[must_use]
    pub fn new(upstreams: U, client: C) -> Self {
        Self {
            upstreams,
            client,
            url_path_getter: Box::new(default_url_path_getter),
            url_query_getter: Box::new(default_url_query_getter),
            host_header_getter: Box::new(default_host_header_getter),
            client_ip_forwarding_enabled: false,
        }
    }

    /// Create new `Proxy` with upstreams list and enable x-forwarded-for header.
    pub fn with_client_ip_forwarding(upstreams: U, client: C) -> Self {
        Self {
            upstreams,
            client,
            url_path_getter: Box::new(default_url_path_getter),
            url_query_getter: Box::new(default_url_query_getter),
            host_header_getter: Box::new(default_host_header_getter),
            client_ip_forwarding_enabled: true,
        }
    }

    /// Set url path getter.
    #[inline]
    #[must_use]
    pub fn url_path_getter<G>(mut self, url_path_getter: G) -> Self
    where
        G: Fn(&Request, &Depot) -> Option<String> + Send + Sync + 'static,
    {
        self.url_path_getter = Box::new(url_path_getter);
        self
    }

    /// Set url query getter.
    #[inline]
    #[must_use]
    pub fn url_query_getter<G>(mut self, url_query_getter: G) -> Self
    where
        G: Fn(&Request, &Depot) -> Option<String> + Send + Sync + 'static,
    {
        self.url_query_getter = Box::new(url_query_getter);
        self
    }

    /// Set host header query getter.
    #[inline]
    #[must_use]
    pub fn host_header_getter<G>(mut self, host_header_getter: G) -> Self
    where
        G: Fn(&Uri, &Request, &Depot) -> Option<String> + Send + Sync + 'static,
    {
        self.host_header_getter = Box::new(host_header_getter);
        self
    }

    /// Get upstreams list.
    #[inline]
    pub fn upstreams(&self) -> &U {
        &self.upstreams
    }
    /// Get upstreams mutable list.
    #[inline]
    pub fn upstreams_mut(&mut self) -> &mut U {
        &mut self.upstreams
    }

    /// Get client reference.
    #[inline]
    pub fn client(&self) -> &C {
        &self.client
    }
    /// Get client mutable reference.
    #[inline]
    pub fn client_mut(&mut self) -> &mut C {
        &mut self.client
    }

    /// Enable x-forwarded-for header prepending.
    #[inline]
    #[must_use]
    pub fn client_ip_forwarding(mut self, enable: bool) -> Self {
        self.client_ip_forwarding_enabled = enable;
        self
    }

    async fn build_proxied_request(
        &self,
        req: &mut Request,
        depot: &Depot,
    ) -> Result<HyperRequest, Error> {
        let upstream = self
            .upstreams
            .elect(req, depot)
            .await
            .map_err(Error::other)?;

        if upstream.is_empty() {
            tracing::error!("upstreams is empty");
            return Err(Error::other("upstreams is empty"));
        }

        let path = encode_url_path(&(self.url_path_getter)(req, depot).unwrap_or_default());
        let query = (self.url_query_getter)(req, depot);
        let rest = if let Some(query) = query {
            if query.starts_with('?') {
                format!("{path}{query}")
            } else {
                format!("{path}?{query}")
            }
        } else {
            path
        };
        let forward_url = if upstream.ends_with('/') && rest.starts_with('/') {
            format!("{}{}", upstream.trim_end_matches('/'), rest)
        } else if upstream.ends_with('/') || rest.starts_with('/') {
            format!("{upstream}{rest}")
        } else if rest.is_empty() {
            upstream.to_owned()
        } else {
            format!("{upstream}/{rest}")
        };
        let forward_url = url::Url::parse(&forward_url).map_err(|e| {
            Error::other(format!("url::Url::parse failed for '{forward_url}': {e}"))
        })?;
        let forward_url: Uri = forward_url
            .as_str()
            .parse()
            .map_err(|e| Error::other(format!("Uri::parse failed for '{forward_url}': {e}")))?;
        let mut build = hyper::Request::builder()
            .method(req.method())
            .uri(&forward_url);
        for (key, value) in req.headers() {
            if key != HOST {
                build = build.header(key, value);
            }
        }
        if let Some(host_value) = (self.host_header_getter)(&forward_url, req, depot) {
            match HeaderValue::from_str(&host_value) {
                Ok(host_value) => {
                    build = build.header(HOST, host_value);
                }
                Err(e) => {
                    tracing::error!(error = ?e, "invalid host header value");
                }
            }
        }

        if self.client_ip_forwarding_enabled {
            let xff_header_name = HeaderName::from_static(X_FORWARDER_FOR_HEADER_NAME);
            let current_xff = req.headers().get(&xff_header_name);

            #[cfg(test)]
            let system_ip_addr = match req.remote_addr() {
                SocketAddr::IPv6(_) => Some(IpAddr::from(Ipv6Addr::new(
                    0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8,
                ))),
                _ => Some(IpAddr::from(Ipv4Addr::new(101, 102, 103, 104))),
            };

            #[cfg(not(test))]
            let system_ip_addr = match req.remote_addr() {
                SocketAddr::IPv6(_) => local_ipv6().ok(),
                _ => local_ip().ok(),
            };

            if let Some(system_ip_addr) = system_ip_addr {
                let forwarded_addr = system_ip_addr.to_string();

                let xff_value = match current_xff {
                    Some(current_xff) => match current_xff.to_str() {
                        Ok(current_xff) => format!("{forwarded_addr}, {current_xff}"),
                        _ => forwarded_addr.clone(),
                    },
                    None => forwarded_addr.clone(),
                };

                let xff_header_halue = match HeaderValue::from_str(xff_value.as_str()) {
                    Ok(xff_header_halue) => Some(xff_header_halue),
                    Err(_) => match HeaderValue::from_str(forwarded_addr.as_str()) {
                        Ok(xff_header_halue) => Some(xff_header_halue),
                        Err(e) => {
                            tracing::error!(error = ?e, "invalid x-forwarded-for header value");
                            None
                        }
                    },
                };

                if let Some(xff) = xff_header_halue
                    && let Some(headers) = build.headers_mut()
                {
                    headers.insert(&xff_header_name, xff);
                }
            }
        }

        build.body(req.take_body()).map_err(Error::other)
    }
}

#[async_trait]
impl<U, C> Handler for Proxy<U, C>
where
    U: Upstreams,
    U::Error: Into<BoxedError>,
    C: Client,
{
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        match self.build_proxied_request(req, depot).await {
            Ok(proxied_request) => {
                match self
                    .client
                    .execute(proxied_request, req.extensions_mut().remove())
                    .await
                {
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
                        res.status_code(status);
                        for name in headers.keys() {
                            for value in headers.get_all(name) {
                                res.headers.append(name, value.to_owned());
                            }
                        }
                        res.body(body);
                    }
                    Err(e) => {
                        tracing::error!( error = ?e, uri = ?req.uri(), "get response data failed: {}", e);
                        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "build proxied request failed");
            }
        }
    }
}
#[inline]
#[allow(dead_code)]
fn get_upgrade_type(headers: &HeaderMap) -> Option<&str> {
    if headers
        .get(&CONNECTION)
        .map(|value| {
            value
                .to_str()
                .unwrap_or_default()
                .split(',')
                .any(|e| e.trim() == UPGRADE)
        })
        .unwrap_or(false)
        && let Some(upgrade_value) = headers.get(&UPGRADE)
    {
        tracing::debug!(
            "found upgrade header with value: {:?}",
            upgrade_value.to_str()
        );
        return upgrade_value.to_str().ok();
    }

    None
}

// Unit tests for Proxy
#[cfg(test)]
mod tests {
    use super::*;

    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
    use std::str::FromStr;

    #[test]
    fn test_encode_url_path() {
        let path = "/test/path";
        let encoded_path = encode_url_path(path);
        assert_eq!(encoded_path, "/test/path");
    }

    #[test]
    fn test_get_upgrade_type() {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION, HeaderValue::from_static("upgrade"));
        headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
        let upgrade_type = get_upgrade_type(&headers);
        assert_eq!(upgrade_type, Some("websocket"));
    }

    #[test]
    fn test_host_header_handling() {
        let uri = Uri::from_str("http://host.tld/test").unwrap();
        let mut req = Request::new();
        let depot = Depot::new();

        assert_eq!(
            default_host_header_getter(&uri, &req, &depot),
            Some("host.tld".to_string())
        );

        let uri_with_port = Uri::from_str("http://host.tld:8080/test").unwrap();
        assert_eq!(
            rfc2616_host_header_getter(&uri_with_port, &req, &depot),
            Some("host.tld:8080".to_string())
        );

        let uri_with_http_port = Uri::from_str("http://host.tld:80/test").unwrap();
        assert_eq!(
            rfc2616_host_header_getter(&uri_with_http_port, &req, &depot),
            Some("host.tld".to_string())
        );

        let uri_with_https_port = Uri::from_str("https://host.tld:443/test").unwrap();
        assert_eq!(
            rfc2616_host_header_getter(&uri_with_https_port, &req, &depot),
            Some("host.tld".to_string())
        );

        let uri_with_non_https_scheme_and_https_port =
            Uri::from_str("http://host.tld:443/test").unwrap();
        assert_eq!(
            rfc2616_host_header_getter(&uri_with_non_https_scheme_and_https_port, &req, &depot),
            Some("host.tld:443".to_string())
        );

        req.headers_mut()
            .insert(HOST, HeaderValue::from_static("test.host.tld"));
        assert_eq!(
            preserve_original_host_header_getter(&uri, &req, &depot),
            Some("test.host.tld".to_string())
        );
    }

    #[tokio::test]
    async fn test_client_ip_forwarding() {
        let xff_header_name = HeaderName::from_static(X_FORWARDER_FOR_HEADER_NAME);

        let mut request = Request::new();
        let mut depot = Depot::new();

        // Test functionality not broken
        let proxy_without_forwarding =
            Proxy::new(vec!["http://example.com"], HyperClient::default());

        assert_eq!(proxy_without_forwarding.client_ip_forwarding_enabled, false);

        let proxy_with_forwarding = proxy_without_forwarding.client_ip_forwarding(true);

        assert_eq!(proxy_with_forwarding.client_ip_forwarding_enabled, true);

        let proxy =
            Proxy::with_client_ip_forwarding(vec!["http://example.com"], HyperClient::default());
        assert_eq!(proxy.client_ip_forwarding_enabled, true);

        match proxy.build_proxied_request(&mut request, &mut depot).await {
            Ok(req) => assert_eq!(
                req.headers().get(&xff_header_name),
                Some(&HeaderValue::from_static("101.102.103.104"))
            ),
            _ => assert!(false),
        }

        // Test choosing correct IP version depending on remote address
        *request.remote_addr_mut() =
            SocketAddr::from(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 12345, 0, 0));

        match proxy.build_proxied_request(&mut request, &mut depot).await {
            Ok(req) => assert_eq!(
                req.headers().get(&xff_header_name),
                Some(&HeaderValue::from_static("1:2:3:4:5:6:7:8"))
            ),
            _ => assert!(false),
        }

        *request.remote_addr_mut() = SocketAddr::Unknown;

        match proxy.build_proxied_request(&mut request, &mut depot).await {
            Ok(req) => assert_eq!(
                req.headers().get(&xff_header_name),
                Some(&HeaderValue::from_static("101.102.103.104"))
            ),
            _ => assert!(false),
        }

        // Test IP prepending when XFF header already exists in initial request.
        request.headers_mut().insert(
            &xff_header_name,
            HeaderValue::from_static("10.72.0.1, 127.0.0.1"),
        );
        *request.remote_addr_mut() =
            SocketAddr::from(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 12345));

        match proxy.build_proxied_request(&mut request, &mut depot).await {
            Ok(req) => assert_eq!(
                req.headers().get(&xff_header_name),
                Some(&HeaderValue::from_static(
                    "101.102.103.104, 10.72.0.1, 127.0.0.1"
                ))
            ),
            _ => assert!(false),
        }
    }
}
