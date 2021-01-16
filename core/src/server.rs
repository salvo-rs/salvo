use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::{future, Future};
use hyper::Server as HyperServer;
use tracing;

use super::pick_port;
use crate::catcher;
use crate::http::header::CONTENT_TYPE;
use crate::http::{Mime, Request, Response, ResponseBody, StatusCode};
use crate::routing::{PathState, Router};
use crate::{Catcher, Depot, Protocol};

/// A settings struct containing a set of timeouts which can be applied to a server.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Timeouts {
    /// Controls the timeout for keep alive connections.
    ///
    /// The default is `Some(Duration::from_secs(5))`.
    ///
    /// NOTE: Setting this to None will have the effect of turning off keep alive.
    pub keep_alive: Option<Duration>,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            keep_alive: Some(Duration::from_secs(5)),
        }
    }
}

/// The main `Novel` type: used to mount routes and catchers and launch the
/// application.
pub struct Server {
    pub router: Arc<Router>,
    pub config: Arc<ServerConfig>,
}
pub struct ServerConfig {
    pub timeouts: Timeouts,

    /// Protocol of the incoming requests
    ///
    /// This is automatically set by the `http` and `https` functions, but
    /// can be set if you are manually constructing the hyper `http` instance.
    pub protocol: Protocol,

    /// Default host address to use when none is provided
    ///
    /// When set, this provides a default host for any requests that don't
    /// provide one.  When unset, any request without a host specified
    /// will fail.
    pub local_addr: Option<SocketAddr>,

    pub catchers: Arc<Vec<Box<dyn Catcher>>>,
    pub allowed_media_types: Arc<Vec<Mime>>,
}
impl ServerConfig {
    pub fn new() -> ServerConfig {
        ServerConfig {
            protocol: Protocol::http(),
            local_addr: None,
            timeouts: Timeouts::default(),
            catchers: Arc::new(catcher::defaults::get()),
            allowed_media_types: Arc::new(vec![]),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig::new()
    }
}

impl Server {
    pub fn new(router: Router) -> Server {
        let config = ServerConfig::default();
        Server {
            router: Arc::new(router),
            config: Arc::new(config),
        }
    }

    pub fn with_config(router: Router, config: ServerConfig) -> Server {
        Server {
            router: Arc::new(router),
            config: Arc::new(config),
        }
    }

    pub fn with_addr<T>(router: Router, addr: T) -> Server
    where
        T: ToSocketAddrs,
    {
        let mut config = ServerConfig::default();
        config.local_addr = addr.to_socket_addrs().unwrap().next();
        Server {
            router: Arc::new(router),
            config: Arc::new(config),
        }
    }

    /// Configure a server to use TLS.
    ///
    /// *This function requires the `"tls"` feature.*
    #[cfg(feature = "tls")]
    pub fn tls(self) -> TlsServer<F> {
        TlsServer {
            server: self,
            tls: TlsConfigBuilder::new(),
        }
    }

    pub fn serve(self) -> impl Future<Output = Result<(), hyper::Error>> + Send + 'static {
        let addr: SocketAddr = self.config.local_addr.unwrap_or_else(|| {
            let port = pick_port::pick_unused_port().expect("Pick unused port failed");
            let addr = format!("localhost:{}", port).to_socket_addrs().unwrap().next().unwrap();
            tracing::warn!("Local address is not set, randrom address used.");
            addr
        });
        tracing::info!("Server listening on {:?}", &addr);
        HyperServer::bind(&addr).tcp_keepalive(self.config.timeouts.keep_alive).serve(self)
    }
}
impl<T> hyper::service::Service<T> for Server {
    type Response = HyperHandler;
    type Error = std::io::Error;
    // type Future = Pin<Box<(dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static)>>;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, _: T) -> Self::Future {
        future::ok(HyperHandler {
            router: self.router.clone(),
            config: self.config.clone(),
        })
    }
}
pub struct HyperHandler {
    router: Arc<Router>,
    config: Arc<ServerConfig>,
}
#[allow(clippy::type_complexity)]
impl hyper::service::Service<hyper::Request<hyper::body::Body>> for HyperHandler {
    type Response = hyper::Response<hyper::body::Body>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn call(&mut self, req: hyper::Request<hyper::body::Body>) -> Self::Future {
        let catchers = self.config.catchers.clone();
        let allowed_media_types = self.config.allowed_media_types.clone();
        let mut request = Request::from_hyper(req).unwrap();
        let mut response = Response::new(self.config.clone());
        let mut depot = Depot::new();
        let segments = request
            .url()
            .path_segments()
            .map(|segments| {
                segments
                    .map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().to_string())
                    .filter(|s| !s.contains('/') && *s != "")
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut path_state = PathState::new(segments);
        response.cookies = request.cookies().clone();

        let router = self.router.clone();
        let fut = async move {
            if let Some(dm) = router.detect(&mut request, &mut path_state) {
                request.params = path_state.params;
                for handler in [&dm.befores[..], &[dm.handler], &dm.afters[..]].concat() {
                    handler.handle(&mut request, &mut depot, &mut response).await;
                    if response.is_commited() {
                        break;
                    }
                }
                if !response.is_commited() {
                    response.commit();
                }
            } else {
                response.set_status_code(StatusCode::NOT_FOUND);
            }

            let mut hyper_response = hyper::Response::<hyper::Body>::new(hyper::Body::empty());

            if response.status_code().is_none() {
                if let ResponseBody::None = response.body {
                    response.set_status_code(StatusCode::NOT_FOUND);
                } else {
                    response.set_status_code(StatusCode::OK);
                }
            }
            let status = response.status_code().unwrap();
            let has_error = status.is_client_error() || status.is_server_error();
            if let Some(value) = response.headers().get(CONTENT_TYPE) {
                let mut is_allowed = false;
                if let Ok(value) = value.to_str() {
                    if allowed_media_types.is_empty() {
                        is_allowed = true;
                    } else {
                        let ctype: Result<Mime, _> = value.parse();
                        if let Ok(ctype) = ctype {
                            for mime in &*allowed_media_types {
                                if mime.type_() == ctype.type_() && mime.subtype() == ctype.subtype() {
                                    is_allowed = true;
                                    break;
                                }
                            }
                        }
                    }
                }
                if !is_allowed {
                    response.set_status_code(StatusCode::UNSUPPORTED_MEDIA_TYPE);
                }
            } else {
                tracing::warn!(
                    url = request.url().as_str(),
                    method = request.method().as_str(),
                    "Http response content type header is not set"
                );
            }
            if let ResponseBody::None = response.body {
                if has_error {
                    for catcher in &*catchers {
                        if catcher.catch(&request, &mut response) {
                            break;
                        }
                    }
                }
            }
            response.write_back(&mut request, &mut hyper_response).await;
            Ok(hyper_response)
        };
        Box::pin(fut)
    }
}

// modified from https://github.com/seanmonstar/warp/blob/master/src/server.rs
#[cfg(feature = "tls")]
impl<F> TlsServer<F>
where
    F: Filter + Clone + Send + Sync + 'static,
    <F::Future as TryFuture>::Ok: Reply,
    <F::Future as TryFuture>::Error: IsReject,
{
    // TLS config methods

    /// Specify the file path to read the private key.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn key_path(self, path: impl AsRef<Path>) -> Self {
        self.with_tls(|tls| tls.key_path(path))
    }

    /// Specify the file path to read the certificate.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn cert_path(self, path: impl AsRef<Path>) -> Self {
        self.with_tls(|tls| tls.cert_path(path))
    }

    /// Specify the file path to read the trust anchor for optional client authentication.
    ///
    /// Anonymous and authenticated clients will be accepted. If no trust anchor is provided by any
    /// of the `client_auth_` methods, then client authentication is disabled by default.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn client_auth_optional_path(self, path: impl AsRef<Path>) -> Self {
        self.with_tls(|tls| tls.client_auth_optional_path(path))
    }

    /// Specify the file path to read the trust anchor for required client authentication.
    ///
    /// Only authenticated clients will be accepted. If no trust anchor is provided by any of the
    /// `client_auth_` methods, then client authentication is disabled by default.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn client_auth_required_path(self, path: impl AsRef<Path>) -> Self {
        self.with_tls(|tls| tls.client_auth_required_path(path))
    }

    /// Specify the in-memory contents of the private key.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn key(self, key: impl AsRef<[u8]>) -> Self {
        self.with_tls(|tls| tls.key(key.as_ref()))
    }

    /// Specify the in-memory contents of the certificate.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn cert(self, cert: impl AsRef<[u8]>) -> Self {
        self.with_tls(|tls| tls.cert(cert.as_ref()))
    }

    /// Specify the in-memory contents of the trust anchor for optional client authentication.
    ///
    /// Anonymous and authenticated clients will be accepted. If no trust anchor is provided by any
    /// of the `client_auth_` methods, then client authentication is disabled by default.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn client_auth_optional(self, trust_anchor: impl AsRef<[u8]>) -> Self {
        self.with_tls(|tls| tls.client_auth_optional(trust_anchor.as_ref()))
    }

    /// Specify the in-memory contents of the trust anchor for required client authentication.
    ///
    /// Only authenticated clients will be accepted. If no trust anchor is provided by any of the
    /// `client_auth_` methods, then client authentication is disabled by default.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn client_auth_required(self, trust_anchor: impl AsRef<[u8]>) -> Self {
        self.with_tls(|tls| tls.client_auth_required(trust_anchor.as_ref()))
    }

    /// Specify the DER-encoded OCSP response.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn ocsp_resp(self, resp: impl AsRef<[u8]>) -> Self {
        self.with_tls(|tls| tls.ocsp_resp(resp.as_ref()))
    }

    fn with_tls<Func>(self, func: Func) -> Self
    where
        Func: FnOnce(TlsConfigBuilder) -> TlsConfigBuilder,
    {
        let TlsServer { server, tls } = self;
        let tls = func(tls);
        TlsServer { server, tls }
    }

    // Server run methods

    /// Run this `TlsServer` forever on the current thread.
    ///
    /// *This function requires the `"tls"` feature.*
    pub async fn run(self, addr: impl Into<SocketAddr>) {
        let (addr, fut) = self.bind_ephemeral(addr);
        let span = tracing::info_span!("TlsServer::run", %addr);
        tracing::info!(parent: &span, "listening on https://{}", addr);

        fut.instrument(span).await;
    }

    /// Bind to a socket address, returning a `Future` that can be
    /// executed on a runtime.
    ///
    /// *This function requires the `"tls"` feature.*
    pub async fn bind(self, addr: impl Into<SocketAddr>) {
        let (_, fut) = self.bind_ephemeral(addr);
        fut.await;
    }

    /// Bind to a possibly ephemeral socket address.
    ///
    /// Returns the bound address and a `Future` that can be executed on
    /// any runtime.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn bind_ephemeral(
        self,
        addr: impl Into<SocketAddr>,
    ) -> (SocketAddr, impl Future<Output = ()> + 'static) {
        let (addr, srv) = bind!(tls: self, addr);
        let srv = srv.map(|result| {
            if let Err(err) = result {
                tracing::error!("server error: {}", err)
            }
        });

        (addr, srv)
    }

    /// Create a server with graceful shutdown signal.
    ///
    /// When the signal completes, the server will start the graceful shutdown
    /// process.
    ///
    /// *This function requires the `"tls"` feature.*
    pub fn bind_with_graceful_shutdown(
        self,
        addr: impl Into<SocketAddr> + 'static,
        signal: impl Future<Output = ()> + Send + 'static,
    ) -> (SocketAddr, impl Future<Output = ()> + 'static) {
        let (addr, srv) = bind!(tls: self, addr);

        let fut = srv.with_graceful_shutdown(signal).map(|result| {
            if let Err(err) = result {
                tracing::error!("server error: {}", err)
            }
        });
        (addr, fut)
    }
}

#[cfg(feature = "tls")]
impl<F> ::std::fmt::Debug for TlsServer<F>
where
    F: ::std::fmt::Debug,
{
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_struct("TlsServer")
            .field("server", &self.server)
            .finish()
    }
}
