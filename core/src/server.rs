use std::error::Error as StdError;
use std::future::Future;
use std::net::SocketAddr;
#[cfg(feature = "tls")]
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use futures::{future, TryStream, TryStreamExt};
use hyper::server::accept::{self, Accept};
use hyper::server::conn::AddrIncoming;
use hyper::Server as HyperServer;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::catcher;
use crate::http::header::CONTENT_TYPE;
use crate::http::{Mime, Request, Response, StatusCode};
use crate::routing::{PathState, Router};
#[cfg(feature = "tls")]
use crate::tls::{TlsAcceptor, TlsConfigBuilder};
use crate::{Catcher, Depot};

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
    fn create_bind_hyper_server(self, addr: impl Into<SocketAddr>) -> Result<(SocketAddr, hyper::Server<AddrIncoming, Self>), hyper::Error> {
        let addr = addr.into();
        let mut incoming = AddrIncoming::bind(&addr)?;
        incoming.set_nodelay(true);
        let srv = HyperServer::builder(incoming).serve(self);
        Ok((addr, srv))
    }

    #[inline]
    fn create_bind_incoming_hyper_server<S>(
        self,
        incoming: S,
    ) -> Result<hyper::Server<impl Accept<Conn = S::Ok, Error = S::Error>, Self>, hyper::Error>
    where
        S: TryStream + Send,
        S::Ok: AsyncRead + AsyncWrite + Send + 'static + Unpin,
        S::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        let srv = HyperServer::builder(accept::from_stream(incoming.into_stream())).serve(self);
        Ok(srv)
    }

    /// Bind to a socket address, returning a `Future` that can be
    /// executed on any runtime.
    ///
    /// # Panics
    ///
    /// Panics if we are unable to bind to the provided address.
    pub async fn bind(self, addr: impl Into<SocketAddr> + 'static) {
        self.try_bind(addr).await.unwrap();
    }

    /// Bind to a socket address, returning a `Future` that can be
    /// executed on any runtime.
    ///
    /// In case we are unable to bind to the specified address, resolves to an
    /// error and logs the reason.
    pub async fn try_bind(self, addr: impl Into<SocketAddr>) -> Result<SocketAddr, hyper::Error> {
        let (addr, srv) = self.create_bind_hyper_server(addr)?;
        tracing::info!("listening with socket addr: {}", addr);
        if let Err(err) = srv.await {
            tracing::error!("server error: {}", err);
            Err(err)
        } else {
            Ok(addr)
        }
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
    /// use salvo_core::prelude::*;
    /// use tokio::sync::oneshot;
    ///
    /// #[fn_handler]
    /// async fn hello_world(res: &mut Response) {
    ///     res.render_plain_text("Hello World!");
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, rx) = oneshot::channel();
    ///     let router = Router::new().get(hello_world);
    ///     let server = Server::new(router).bind_with_graceful_shutdown(([0, 0, 0, 0], 3131), async {
    ///             rx.await.ok();
    ///     });
    ///
    ///     // Spawn the server into a runtime
    ///     tokio::task::spawn(server);
    ///
    ///     // Later, start the shutdown...
    ///     let _ = tx.send(());
    /// }
    /// ```
    pub async fn bind_with_graceful_shutdown(self, addr: impl Into<SocketAddr> + 'static, signal: impl Future<Output = ()> + Send + 'static) {
        self.try_bind_with_graceful_shutdown(addr, signal).await.unwrap();
    }

    /// Create a server with graceful shutdown signal.
    ///
    /// When the signal completes, the server will start the graceful shutdown
    /// process.
    pub async fn try_bind_with_graceful_shutdown(
        self,
        addr: impl Into<SocketAddr> + 'static,
        signal: impl Future<Output = ()> + Send + 'static,
    ) -> Result<SocketAddr, hyper::Error> {
        let (addr, srv) = self.create_bind_hyper_server(addr)?;
        if let Err(err) = srv.with_graceful_shutdown(signal).await {
            tracing::error!("server error: {}", err);
            Err(err)
        } else {
            Ok(addr)
        }
    }

    /// Bind to a stream, returning a `Future` that can be
    /// executed on any runtime.
    ///
    /// # Panics
    ///
    /// Panics if we are unable to bind to the provided address.
    pub async fn bind_incoming<I>(self, incoming: I)
    where
        I: TryStream + Send,
        I::Ok: AsyncRead + AsyncWrite + Send + 'static + Unpin,
        I::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        self.try_bind_incoming(incoming).await.unwrap();
    }

    /// Run this `Server` forever on the current thread with a specific stream
    /// of incoming connections.
    ///
    /// This can be used for Unix Domain Sockets, or TLS, etc.
    pub async fn try_bind_incoming<I>(self, incoming: I) -> Result<(), hyper::Error>
    where
        I: TryStream + Send,
        I::Ok: AsyncRead + AsyncWrite + Send + 'static + Unpin,
        I::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        let srv = self.create_bind_incoming_hyper_server(incoming)?;
        tracing::info!("listening with custom incoming");
        if let Err(err) = srv.await {
            tracing::error!("server error: {}", err);
            Err(err)
        } else {
            Ok(())
        }
    }

    pub async fn bind_incoming_with_graceful_shutdown<I>(self, incoming: I, signal: impl Future<Output = ()> + Send + 'static)
    where
        I: TryStream + Send,
        I::Ok: AsyncRead + AsyncWrite + Send + 'static + Unpin,
        I::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        self.try_bind_incoming_with_graceful_shutdown(incoming, signal).await.unwrap();
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
    pub async fn try_bind_incoming_with_graceful_shutdown<I>(
        self,
        incoming: I,
        signal: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), hyper::Error>
    where
        I: TryStream + Send,
        I::Ok: AsyncRead + AsyncWrite + Send + 'static + Unpin,
        I::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        let srv = self.create_bind_incoming_hyper_server(incoming)?;
        tracing::info!("listening with custom incoming");
        if let Err(err) = srv.with_graceful_shutdown(signal).await {
            tracing::error!("server error: {}", err);
            Err(err)
        } else {
            Ok(())
        }
    }

    /// Configure a server to use TLS.
    ///
    /// *This function requires the `"tls"` feature.*
    #[cfg(feature = "tls")]
    pub fn tls(self) -> TlsServer {
        TlsServer {
            server: self,
            builder: TlsConfigBuilder::new(),
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

#[cfg(feature = "tls")]
pub struct TlsServer {
    server: Server,
    builder: TlsConfigBuilder,
}
#[cfg(feature = "tls")]
impl TlsServer {
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
        let TlsServer { server, builder } = self;
        let builder = func(builder);
        TlsServer { server, builder }
    }

    #[inline]
    fn create_bind_hyper_server(self, addr: impl Into<SocketAddr>) -> Result<(SocketAddr, hyper::Server<TlsAcceptor, Server>), crate::Error> {
        let addr = addr.into();
        let TlsServer { server, builder } = self;
        let tls = builder.build().map_err(crate::Error::new)?;
        let mut incoming = AddrIncoming::bind(&addr).map_err(crate::Error::new)?;
        incoming.set_nodelay(true);
        let srv = HyperServer::builder(TlsAcceptor::new(tls, incoming)).serve(server);
        Ok((addr, srv))
    }
    /// Bind to a socket address, returning a `Future` that can be
    /// executed on a runtime.
    ///
    /// *This function requires the `"tls"` feature.*
    pub async fn bind(self, addr: impl Into<SocketAddr> + 'static) {
        self.try_bind(addr).await.unwrap();
    }
    /// Bind to a socket address, returning a `Future` that can be
    /// executed on any runtime.
    ///
    /// In case we are unable to bind to the specified address, resolves to an
    /// error and logs the reason.
    ///
    /// *This function requires the `"tls"` feature.*
    pub async fn try_bind(self, addr: impl Into<SocketAddr>) -> Result<SocketAddr, crate::Error> {
        let (addr, srv) = self.create_bind_hyper_server(addr)?;
        tracing::info!("tls listening with socket addr");
        if let Err(err) = srv.await {
            tracing::error!("server error: {}", err);
            Err(crate::Error::new(err))
        } else {
            Ok(addr)
        }
    }
    /// Create a server with graceful shutdown signal.
    ///
    /// When the signal completes, the server will start the graceful shutdown
    /// process.
    ///
    /// *This function requires the `"tls"` feature.*
    pub async fn try_bind_with_graceful_shutdown(
        self,
        addr: impl Into<SocketAddr> + 'static,
        signal: impl Future<Output = ()> + Send + 'static,
    ) -> Result<SocketAddr, crate::Error> {
        let (addr, srv) = self.create_bind_hyper_server(addr)?;
        tracing::info!("tls listening with socket addr");
        if let Err(err) = srv.with_graceful_shutdown(signal).await {
            tracing::error!("server error: {}", err);
            Err(crate::Error::new(err))
        } else {
            Ok(addr)
        }
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
        let segments = if let Some(path) = path.strip_prefix('/') { path.split('/') } else { path.split('/') };
        let segments = segments
            .map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().to_string())
            .filter(|s| !s.contains('/') && !s.is_empty())
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
                if response.body.is_none() {
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
            if response.body.is_none() && has_error {
                for catcher in &*catchers {
                    if catcher.catch(&request, &mut response) {
                        break;
                    }
                }
            }
            response.write_back(&mut request, &mut hyper_response).await;
            Ok(hyper_response)
        };
        Box::pin(fut)
    }
}
