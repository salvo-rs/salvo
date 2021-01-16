use std::error::Error as StdError;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use futures::{future, FutureExt, TryStream, TryStreamExt};
use hyper::server::conn::AddrIncoming;
use hyper::Server as HyperServer;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing;
use tracing_futures::Instrument;

use crate::catcher;
use crate::http::header::CONTENT_TYPE;
use crate::http::{Mime, Request, Response, ResponseBody, StatusCode};
use crate::routing::{PathState, Router};
use crate::{Catcher, Depot};

macro_rules! addr_incoming {
    ($addr:expr) => {{
        let mut incoming = AddrIncoming::bind($addr)?;
        incoming.set_nodelay(true);
        let addr = incoming.local_addr();
        (addr, incoming)
    }};
}

macro_rules! bind_inner {
    ($this:ident, $addr:expr) => {{
        let (addr, incoming) = addr_incoming!($addr);
        let srv = HyperServer::builder(incoming).serve($this);
        Ok::<_, hyper::Error>((addr, srv))
    }};

    (tls: $this:ident, $addr:expr) => {{
        let (addr, incoming) = addr_incoming!($addr);
        let tls = $this.tls.build()?;
        let srv = HyperServer::builder(crate::tls::TlsAcceptor::new(tls, incoming)).serve($this);
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((addr, srv))
    }};
}

macro_rules! bind {
    ($this:ident, $addr:expr) => {{
        let addr = $addr.into();
        (|addr| bind_inner!($this, addr))(&addr).unwrap_or_else(|e| {
            panic!("error binding to {}: {}", addr, e);
        })
    }};

    (tls: $this:ident, $addr:expr) => {{
        let addr = $addr.into();
        (|addr| bind_inner!(tls: $this, addr))(&addr).unwrap_or_else(|e| {
            panic!("error binding to {}: {}", addr, e);
        })
    }};
}

macro_rules! try_bind {
    ($this:ident, $addr:expr) => {{
        (|addr| bind_inner!($this, addr))($addr)
    }};

    (tls: $this:ident, $addr:expr) => {{
        (|addr| bind_inner!(tls: $this, addr))($addr)
    }};
}

pub struct Server {
    pub router: Arc<Router>,
    pub catchers: Arc<Vec<Box<dyn Catcher>>>,
    pub allowed_media_types: Arc<Vec<Mime>>,
}

impl Server {
    pub fn new(router: Router) -> Server {
        Server {
            router: Arc::new(router),
            catchers: Arc::new(catcher::defaults::get()),
            allowed_media_types: Arc::new(vec![]),
        }
    }

    /// Run this `Server` forever on the current thread.
    pub async fn run(self, addr: impl Into<SocketAddr>) {
        let (addr, fut) = self.bind_ephemeral(addr);
        let span = tracing::info_span!("Server::run", ?addr);
        tracing::info!(parent: &span, "listening on http://{}", addr);

        fut.instrument(span).await;
    }

    /// Run this `Server` forever on the current thread with a specific stream
    /// of incoming connections.
    ///
    /// This can be used for Unix Domain Sockets, or TLS, etc.
    pub async fn run_incoming<I>(self, incoming: I)
    where
        I: TryStream + Send,
        I::Ok: AsyncRead + AsyncWrite + Send + 'static + Unpin,
        I::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        let srv = HyperServer::builder(hyper::server::accept::from_stream(incoming.into_stream())).serve(self);

        tracing::info!("listening with custom incoming");
        if let Err(err) = srv.await {
            tracing::error!("server error: {}", err);
        }
    }

    /// Bind to a socket address, returning a `Future` that can be
    /// executed on any runtime.
    ///
    /// # Panics
    ///
    /// Panics if we are unable to bind to the provided address.
    pub fn bind(self, addr: impl Into<SocketAddr> + 'static) -> impl Future<Output = ()> + 'static {
        let (_, fut) = self.bind_ephemeral(addr);
        fut
    }

    /// Bind to a socket address, returning a `Future` that can be
    /// executed on any runtime.
    ///
    /// In case we are unable to bind to the specified address, resolves to an
    /// error and logs the reason.
    pub async fn try_bind(self, addr: impl Into<SocketAddr>) {
        let addr = addr.into();
        let srv = match try_bind!(self, &addr) {
            Ok((_, srv)) => srv,
            Err(err) => {
                tracing::error!("error binding to {}: {}", addr, err);
                return;
            }
        };

        srv.map(|result| {
            if let Err(err) = result {
                tracing::error!("server error: {}", err)
            }
        })
        .await;
    }

    /// Bind to a possibly ephemeral socket address.
    ///
    /// Returns the bound address and a `Future` that can be executed on
    /// any runtime.
    ///
    /// # Panics
    ///
    /// Panics if we are unable to bind to the provided address.
    pub fn bind_ephemeral(self, addr: impl Into<SocketAddr>) -> (SocketAddr, impl Future<Output = ()> + 'static) {
        let (addr, srv) = bind!(self, addr);
        let srv = srv.map(|result| {
            if let Err(err) = result {
                tracing::error!("server error: {}", err)
            }
        });

        (addr, srv)
    }

    /// Tried to bind a possibly ephemeral socket address.
    ///
    /// Returns a `Result` which fails in case we are unable to bind with the
    /// underlying error.
    ///
    /// Returns the bound address and a `Future` that can be executed on
    /// any runtime.
    pub fn try_bind_ephemeral(self, addr: impl Into<SocketAddr>) -> Result<(SocketAddr, impl Future<Output = ()> + 'static), crate::Error> {
        let addr = addr.into();
        let (addr, srv) = try_bind!(self, &addr).map_err(crate::Error::new)?;
        let srv = srv.map(|result| {
            if let Err(err) = result {
                tracing::error!("server error: {}", err)
            }
        });

        Ok((addr, srv))
    }

    /// Create a server with graceful shutdown signal.
    ///
    /// When the signal completes, the server will start the graceful shutdown
    /// process.
    ///
    /// Returns the bound address and a `Future` that can be executed on
    /// any runtime.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use salvo::Filter;
    /// use futures::future::TryFutureExt;
    /// use tokio::sync::oneshot;
    ///
    /// # fn main() {
    /// let routes = salvo::any()
    ///     .map(|| "Hello, World!");
    ///
    /// let (tx, rx) = oneshot::channel();
    ///
    /// let (addr, server) = salvo::serve(routes)
    ///     .bind_with_graceful_shutdown(([127, 0, 0, 1], 3030), async {
    ///          rx.await.ok();
    ///     });
    ///
    /// // Spawn the server into a runtime
    /// tokio::task::spawn(server);
    ///
    /// // Later, start the shutdown...
    /// let _ = tx.send(());
    /// # }
    /// ```
    pub fn bind_with_graceful_shutdown(
        self,
        addr: impl Into<SocketAddr> + 'static,
        signal: impl Future<Output = ()> + Send + 'static,
    ) -> (SocketAddr, impl Future<Output = ()> + 'static) {
        let (addr, srv) = bind!(self, addr);
        let fut = srv.with_graceful_shutdown(signal).map(|result| {
            if let Err(err) = result {
                tracing::error!("server error: {}", err)
            }
        });
        (addr, fut)
    }

    /// Create a server with graceful shutdown signal.
    ///
    /// When the signal completes, the server will start the graceful shutdown
    /// process.
    pub fn try_bind_with_graceful_shutdown(
        self,
        addr: impl Into<SocketAddr> + 'static,
        signal: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(SocketAddr, impl Future<Output = ()> + 'static), crate::Error> {
        let addr = addr.into();
        let (addr, srv) = try_bind!(self, &addr).map_err(crate::Error::new)?;
        let srv = srv.with_graceful_shutdown(signal).map(|result| {
            if let Err(err) = result {
                tracing::error!("server error: {}", err)
            }
        });

        Ok((addr, srv))
    }

    /// Setup this `Server` with a specific stream of incoming connections.
    ///
    /// This can be used for Unix Domain Sockets, or TLS, etc.
    ///
    /// Returns a `Future` that can be executed on any runtime.
    pub fn serve_incoming<I>(self, incoming: I) -> impl Future<Output = Result<(), hyper::Error>> + Send
    where
        I: TryStream + Send,
        I::Ok: AsyncRead + AsyncWrite + Send + 'static + Unpin,
        I::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        HyperServer::builder(hyper::server::accept::from_stream(incoming.into_stream())).serve(self)
    }

    /// Setup this `Server` with a specific stream of incoming connections and a
    /// signal to initiate graceful shutdown.
    ///
    /// This can be used for Unix Domain Sockets, or TLS, etc.
    ///
    /// When the signal completes, the server will start the graceful shutdown
    /// process.
    ///
    /// Returns a `Future` that can be executed on any runtime.
    pub fn serve_incoming_with_graceful_shutdown<I>(
        self,
        incoming: I,
        signal: impl Future<Output = ()> + Send + 'static,
    ) -> impl Future<Output = Result<(), hyper::Error>> + Send
    where
        I: TryStream + Send,
        I::Ok: AsyncRead + AsyncWrite + Send + 'static + Unpin,
        I::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        HyperServer::builder(hyper::server::accept::from_stream(incoming.into_stream()))
            .serve(self)
            .with_graceful_shutdown(signal)
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
            catchers: self.catchers.clone(),
            allowed_media_types: self.allowed_media_types.clone(),
        })
    }
}
pub struct HyperHandler {
    router: Arc<Router>,
    catchers: Arc<Vec<Box<dyn Catcher>>>,
    allowed_media_types: Arc<Vec<Mime>>,
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
        let catchers = self.catchers.clone();
        let allowed_media_types = self.allowed_media_types.clone();
        let mut request = Request::from_hyper(req).unwrap();
        let mut response = Response::new(allowed_media_types.clone());
        let mut depot = Depot::new();
        let path = request.uri().path();
        let segments = if path.starts_with('/') { path[1..].split('/') } else { path.split('/') };
        let segments = segments
            .map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().to_string())
            .filter(|s| !s.contains('/') && *s != "")
            .collect::<Vec<_>>();
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
                    uri = ?request.uri(),
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

// modified from https://github.com/kenorld/salvo/blob/master/src/server.rs
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
    pub fn bind_ephemeral(self, addr: impl Into<SocketAddr>) -> (SocketAddr, impl Future<Output = ()> + 'static) {
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
        f.debug_struct("TlsServer").field("server", &self.server).finish()
    }
}
