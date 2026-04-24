#![cfg_attr(test, allow(clippy::unwrap_used))]
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
#[cfg(test)]
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use hyper::upgrade::OnUpgrade;
#[cfg(not(test))]
use local_ip_address::{local_ip, local_ipv6};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use salvo_core::conn::SocketAddr;
use salvo_core::http::header::{CONNECTION, HOST, HeaderMap, HeaderName, HeaderValue, UPGRADE};
use salvo_core::http::uri::Uri;
use salvo_core::http::{ReqBody, ResBody, StatusCode};
use salvo_core::routing::normalize_url_path;
use salvo_core::{BoxedError, Depot, Error, FlowCtrl, Handler, Request, Response, async_trait};

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
const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

const QUERY_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'`');
const PATH_ENCODE_SET: &AsciiSet = &QUERY_ENCODE_SET
    .add(b'?')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// Encode url path. This can be used when build your custom url path getter.
#[inline]
pub(crate) fn encode_url_path(path: &str) -> String {
    path.split('/')
        .map(|s| utf8_percent_encode(s, PATH_ENCODE_SET).to_string())
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
    req.params().tail().map(str::to_owned)
}

fn contains_ambiguous_path_escape(path: &str) -> bool {
    let bytes = path.as_bytes();
    let mut index = 0;
    while index + 2 < bytes.len() {
        if bytes[index] == b'%' {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                let decoded = high << 4 | low;
                if matches!(decoded, b'.' | b'/' | b'\\' | b'%') {
                    return true;
                }
                index += 3;
                continue;
            }
        }
        index += 1;
    }
    false
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
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

/// RFC2616 complieant host header getter. This getter will get the host header from request uri,
/// and add port if it's not default port. Falls back to default upon any forward URI parse error.
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

/// Preserve original host header getter. Propagates the original request host header to the proxied
/// request.
pub fn preserve_original_host_header_getter(
    forward_uri: &Uri,
    req: &Request,
    _depot: &Depot,
) -> Option<String> {
    if let Some(host_header) = req.headers().get(HOST)
        && let Ok(host) = host_header.to_str()
    {
        return Some(host.to_owned());
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
    /// Flag to reject ambiguous percent-encoded path characters before proxying.
    pub strict_path_normalization_enabled: bool,
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
            strict_path_normalization_enabled: false,
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
            strict_path_normalization_enabled: false,
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

    /// Enable or disable strict path normalization.
    ///
    /// When enabled, the proxy rejects paths that still contain percent-encoded `.`, `/`, `\`,
    /// or `%` characters after Salvo routing has extracted the path tail. This is useful when the
    /// proxy is used as a security boundary and the upstream server may perform another decode
    /// pass.
    #[inline]
    #[must_use]
    pub fn strict_path_normalization(mut self, enable: bool) -> Self {
        self.strict_path_normalization_enabled = enable;
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

        let path = (self.url_path_getter)(req, depot).unwrap_or_default();
        if self.strict_path_normalization_enabled && contains_ambiguous_path_escape(&path) {
            return Err(Error::other("ambiguous percent-encoded path"));
        }
        let path = encode_url_path(&normalize_url_path(&path));
        let query = (self.url_query_getter)(req, depot);
        let rest = if let Some(query) = query {
            if let Some(stripped) = query.strip_prefix('?') {
                format!("{path}?{}", utf8_percent_encode(stripped, QUERY_ENCODE_SET))
            } else {
                format!("{path}?{}", utf8_percent_encode(&query, QUERY_ENCODE_SET))
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
        let forward_url: Uri = TryFrom::try_from(forward_url).map_err(Error::other)?;
        let mut build = hyper::Request::builder()
            .method(req.method())
            .uri(&forward_url);
        let connection_headers = connection_header_names(req.headers());
        let upgrade_type = get_upgrade_type(req.headers()).map(str::to_owned);
        for (key, value) in req.headers() {
            if key != HOST && !is_hop_by_hop_header(key, &connection_headers) {
                build = build.header(key, value);
            }
        }
        if let Some(upgrade_type) = upgrade_type {
            build = build.header(CONNECTION, HeaderValue::from_static("upgrade"));
            match HeaderValue::from_str(&upgrade_type) {
                Ok(upgrade_type) => {
                    build = build.header(UPGRADE, upgrade_type);
                }
                Err(e) => {
                    tracing::error!(error = ?e, "invalid upgrade header value");
                }
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
                res.status_code(StatusCode::BAD_REQUEST);
            }
        }
    }
}

fn connection_header_names(headers: &HeaderMap) -> Vec<HeaderName> {
    headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| HeaderName::from_bytes(name.trim().as_bytes()).ok())
        .collect()
}

fn is_hop_by_hop_header(name: &HeaderName, connection_headers: &[HeaderName]) -> bool {
    HOP_BY_HOP_HEADERS
        .iter()
        .any(|hop_header| name.as_str().eq_ignore_ascii_case(hop_header))
        || connection_headers.iter().any(|header| header == name)
}

#[inline]
#[allow(dead_code)]
fn get_upgrade_type(headers: &HeaderMap) -> Option<&str> {
    if connection_header_names(headers)
        .iter()
        .any(|name| name == UPGRADE)
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
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
    use std::str::FromStr;

    use futures_util::{SinkExt, StreamExt};
    use salvo_core::conn::{Acceptor, Listener};
    use salvo_core::prelude::{Router, Server, StatusError, TcpListener, handler};
    use salvo_extra::websocket::WebSocketUpgrade;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::protocol::Role;

    use super::*;

    #[handler]
    async fn websocket_echo(req: &mut Request, res: &mut Response) -> Result<(), StatusError> {
        WebSocketUpgrade::new()
            .upgrade(req, res, |mut ws| async move {
                while let Some(message) = ws.recv().await {
                    let Ok(message) = message else {
                        return;
                    };
                    if ws.send(message).await.is_err() {
                        return;
                    }
                }
            })
            .await
    }

    async fn spawn_server(router: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let handle = tokio::spawn(async move {
            Server::new(acceptor).serve(router).await;
        });
        (addr, handle)
    }

    #[test]
    fn test_encode_url_path() {
        let path = "/test/path";
        let encoded_path = encode_url_path(path);
        assert_eq!(encoded_path, "/test/path");
    }

    #[test]
    fn test_default_url_path_getter_uses_raw_tail() {
        let mut request = Request::new();
        request
            .params_mut()
            .insert("**rest", "guide/../index.html".to_owned());
        let depot = Depot::new();

        assert_eq!(
            default_url_path_getter(&request, &depot).as_deref(),
            Some("guide/../index.html")
        );
    }

    #[test]
    fn test_contains_ambiguous_path_escape() {
        assert!(contains_ambiguous_path_escape("%2e%2e/admin"));
        assert!(contains_ambiguous_path_escape("api%2Fadmin"));
        assert!(contains_ambiguous_path_escape("api%5cadmin"));
        assert!(contains_ambiguous_path_escape("%252e%252e/admin"));
        assert!(!contains_ambiguous_path_escape("guide.v1/index.html"));
        assert!(!contains_ambiguous_path_escape("files/%20space"));
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
    fn test_get_upgrade_type_checks_all_connection_headers() {
        let mut headers = HeaderMap::new();
        headers.append(CONNECTION, HeaderValue::from_static("keep-alive"));
        headers.append(CONNECTION, HeaderValue::from_static("Upgrade"));
        headers.insert(UPGRADE, HeaderValue::from_static("websocket"));

        let upgrade_type = get_upgrade_type(&headers);

        assert_eq!(upgrade_type, Some("websocket"));
    }

    #[test]
    fn test_connection_header_names() {
        let mut headers = HeaderMap::new();
        headers.append(CONNECTION, HeaderValue::from_static("keep-alive, x-remove"));
        headers.append(CONNECTION, HeaderValue::from_static("x-second"));

        let names = connection_header_names(&headers);
        assert!(names.contains(&HeaderName::from_static("keep-alive")));
        assert!(names.contains(&HeaderName::from_static("x-remove")));
        assert!(names.contains(&HeaderName::from_static("x-second")));
    }

    #[test]
    fn test_host_header_handling() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let uri = Uri::from_str("http://host.tld/test").unwrap();
        let mut req = Request::new();
        let depot = Depot::new();

        assert_eq!(
            default_host_header_getter(&uri, &req, &depot),
            Some("host.tld".to_owned())
        );

        let uri_with_port = Uri::from_str("http://host.tld:8080/test").unwrap();
        assert_eq!(
            rfc2616_host_header_getter(&uri_with_port, &req, &depot),
            Some("host.tld:8080".to_owned())
        );

        let uri_with_http_port = Uri::from_str("http://host.tld:80/test").unwrap();
        assert_eq!(
            rfc2616_host_header_getter(&uri_with_http_port, &req, &depot),
            Some("host.tld".to_owned())
        );

        let uri_with_https_port = Uri::from_str("https://host.tld:443/test").unwrap();
        assert_eq!(
            rfc2616_host_header_getter(&uri_with_https_port, &req, &depot),
            Some("host.tld".to_owned())
        );

        let uri_with_non_https_scheme_and_https_port =
            Uri::from_str("http://host.tld:443/test").unwrap();
        assert_eq!(
            rfc2616_host_header_getter(&uri_with_non_https_scheme_and_https_port, &req, &depot),
            Some("host.tld:443".to_owned())
        );

        req.headers_mut()
            .insert(HOST, HeaderValue::from_static("test.host.tld"));
        assert_eq!(
            preserve_original_host_header_getter(&uri, &req, &depot),
            Some("test.host.tld".to_owned())
        );
    }

    #[tokio::test]
    async fn test_build_proxied_request_strips_hop_by_hop_headers() {
        let proxy = Proxy::new(vec!["http://example.com"], HyperClient::default());
        let mut request = Request::new();
        let depot = Depot::new();

        request
            .headers_mut()
            .insert(HOST, HeaderValue::from_static("client.example"));
        request
            .headers_mut()
            .insert(CONNECTION, HeaderValue::from_static("keep-alive, x-remove"));
        request.headers_mut().insert(
            HeaderName::from_static("keep-alive"),
            HeaderValue::from_static("timeout=5"),
        );
        request.headers_mut().insert(
            HeaderName::from_static("x-remove"),
            HeaderValue::from_static("secret"),
        );
        request.headers_mut().insert(
            HeaderName::from_static("te"),
            HeaderValue::from_static("trailers"),
        );
        request.headers_mut().insert(
            HeaderName::from_static("transfer-encoding"),
            HeaderValue::from_static("chunked"),
        );
        request.headers_mut().insert(
            HeaderName::from_static("x-keep"),
            HeaderValue::from_static("ok"),
        );

        let proxied = proxy
            .build_proxied_request(&mut request, &depot)
            .await
            .unwrap();

        assert!(proxied.headers().get(CONNECTION).is_none());
        assert!(
            proxied
                .headers()
                .get(HeaderName::from_static("keep-alive"))
                .is_none()
        );
        assert!(
            proxied
                .headers()
                .get(HeaderName::from_static("x-remove"))
                .is_none()
        );
        assert!(
            proxied
                .headers()
                .get(HeaderName::from_static("te"))
                .is_none()
        );
        assert!(
            proxied
                .headers()
                .get(HeaderName::from_static("transfer-encoding"))
                .is_none()
        );
        assert_eq!(
            proxied.headers().get(HeaderName::from_static("x-keep")),
            Some(&HeaderValue::from_static("ok"))
        );
    }

    #[tokio::test]
    async fn test_build_proxied_request_regenerates_upgrade_headers() {
        let proxy = Proxy::new(vec!["http://example.com"], HyperClient::default());
        let mut request = Request::new();
        let depot = Depot::new();

        request
            .headers_mut()
            .insert(CONNECTION, HeaderValue::from_static("x-remove, Upgrade"));
        request
            .headers_mut()
            .insert(UPGRADE, HeaderValue::from_static("websocket"));
        request.headers_mut().insert(
            HeaderName::from_static("x-remove"),
            HeaderValue::from_static("secret"),
        );

        let proxied = proxy
            .build_proxied_request(&mut request, &depot)
            .await
            .unwrap();

        assert_eq!(
            proxied.headers().get(CONNECTION),
            Some(&HeaderValue::from_static("upgrade"))
        );
        assert_eq!(
            proxied.headers().get(UPGRADE),
            Some(&HeaderValue::from_static("websocket"))
        );
        assert!(
            proxied
                .headers()
                .get(HeaderName::from_static("x-remove"))
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_proxy_websocket_connection_with_split_connection_headers() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let upstream_router = Router::with_path("ws").goal(websocket_echo);
        let (upstream_addr, upstream_server) = spawn_server(upstream_router).await;

        let proxy_router = Router::with_path("{**rest}").goal(Proxy::new(
            vec![format!("http://{upstream_addr}")],
            HyperClient::default(),
        ));
        let (proxy_addr, proxy_server) = spawn_server(proxy_router).await;

        let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let request = format!(
            "\
GET /ws HTTP/1.1\r\n\
Host: {proxy_addr}\r\n\
Connection: keep-alive\r\n\
Connection: Upgrade\r\n\
Upgrade: websocket\r\n\
Sec-WebSocket-Version: 13\r\n\
Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = Vec::new();
        let mut buffer = [0; 1024];
        let header_end = loop {
            let read = stream.read(&mut buffer).await.unwrap();
            assert_ne!(
                read, 0,
                "server closed before websocket handshake completed"
            );
            response.extend_from_slice(&buffer[..read]);
            if let Some(position) = response.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
        };
        let extra = response.split_off(header_end);
        let response_head = String::from_utf8_lossy(&response);
        assert!(
            response_head.starts_with("HTTP/1.1 101"),
            "unexpected websocket handshake response: {response_head}"
        );

        let mut websocket = tokio_tungstenite::WebSocketStream::from_partially_read(
            stream,
            extra,
            Role::Client,
            None,
        )
        .await;

        websocket
            .send(Message::text("proxied websocket"))
            .await
            .unwrap();
        let echoed = websocket.next().await.unwrap().unwrap();
        assert_eq!(echoed.into_text().unwrap(), "proxied websocket");

        websocket.close(None).await.unwrap();
        proxy_server.abort();
        upstream_server.abort();
    }

    #[tokio::test]
    async fn test_client_ip_forwarding() {
        let xff_header_name = HeaderName::from_static(X_FORWARDER_FOR_HEADER_NAME);

        let mut request = Request::new();
        let depot = Depot::new();

        // Test functionality not broken
        let proxy_without_forwarding =
            Proxy::new(vec!["http://example.com"], HyperClient::default());

        assert!(!proxy_without_forwarding.client_ip_forwarding_enabled);

        let proxy_with_forwarding = proxy_without_forwarding.client_ip_forwarding(true);

        assert!(proxy_with_forwarding.client_ip_forwarding_enabled);

        let proxy =
            Proxy::with_client_ip_forwarding(vec!["http://example.com"], HyperClient::default());
        assert!(proxy.client_ip_forwarding_enabled);

        match proxy.build_proxied_request(&mut request, &depot).await {
            Ok(req) => assert_eq!(
                req.headers().get(&xff_header_name),
                Some(&HeaderValue::from_static("101.102.103.104"))
            ),
            _ => panic!("expected Ok"),
        }

        // Test choosing correct IP version depending on remote address
        *request.remote_addr_mut() =
            SocketAddr::from(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 12345, 0, 0));

        match proxy.build_proxied_request(&mut request, &depot).await {
            Ok(req) => assert_eq!(
                req.headers().get(&xff_header_name),
                Some(&HeaderValue::from_static("1:2:3:4:5:6:7:8"))
            ),
            _ => panic!("expected Ok"),
        }

        *request.remote_addr_mut() = SocketAddr::Unknown;

        match proxy.build_proxied_request(&mut request, &depot).await {
            Ok(req) => assert_eq!(
                req.headers().get(&xff_header_name),
                Some(&HeaderValue::from_static("101.102.103.104"))
            ),
            _ => panic!("expected Ok"),
        }

        // Test IP prepending when XFF header already exists in initial request.
        request.headers_mut().insert(
            &xff_header_name,
            HeaderValue::from_static("10.72.0.1, 127.0.0.1"),
        );
        *request.remote_addr_mut() =
            SocketAddr::from(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 12345));

        match proxy.build_proxied_request(&mut request, &depot).await {
            Ok(req) => assert_eq!(
                req.headers().get(&xff_header_name),
                Some(&HeaderValue::from_static(
                    "101.102.103.104, 10.72.0.1, 127.0.0.1"
                ))
            ),
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn test_build_proxied_request_unsafe_tail() {
        let mut request = Request::new();
        request.params_mut().insert("**rest", "../admin".to_owned());
        let depot = Depot::new();
        let proxy = Proxy::new(vec!["http://example.com/api"], HyperClient::default());

        let req = proxy
            .build_proxied_request(&mut request, &depot)
            .await
            .unwrap();
        assert_eq!(req.uri().to_string(), "http://example.com/api/admin");
    }

    #[tokio::test]
    async fn test_build_proxied_request_normalizes_safe_tail() {
        let mut request = Request::new();
        request
            .params_mut()
            .insert("**rest", "guide\\index.html".to_owned());
        let depot = Depot::new();
        let proxy = Proxy::new(vec!["http://example.com/api"], HyperClient::default());

        let proxied_request = proxy
            .build_proxied_request(&mut request, &depot)
            .await
            .unwrap();
        assert_eq!(
            proxied_request.uri().to_string(),
            "http://example.com/api/guide/index.html"
        );
    }

    #[tokio::test]
    async fn test_build_proxied_request_preserves_encoded_tail_by_default() {
        let mut request = Request::new();
        request
            .params_mut()
            .insert("**rest", "%2e%2e/secrets/.env".to_owned());
        let depot = Depot::new();
        let proxy = Proxy::new(vec!["http://example.com/api"], HyperClient::default());

        let proxied_request = proxy
            .build_proxied_request(&mut request, &depot)
            .await
            .unwrap();
        assert_eq!(
            proxied_request.uri().to_string(),
            "http://example.com/api/%2e%2e/secrets/.env"
        );
    }

    #[tokio::test]
    async fn test_build_proxied_request_strict_path_normalization_rejects_ambiguous_escapes() {
        for path in [
            "%2e%2e/secrets/.env",
            "api%2fadmin",
            "api%5cadmin",
            "%252e%252e/secrets/.env",
        ] {
            let mut request = Request::new();
            request.params_mut().insert("**rest", path.to_owned());
            let depot = Depot::new();
            let proxy = Proxy::new(vec!["http://example.com/api"], HyperClient::default())
                .strict_path_normalization(true);

            let err = proxy.build_proxied_request(&mut request, &depot).await;
            assert!(err.is_err(), "path should be rejected: {path}");
        }
    }
}
