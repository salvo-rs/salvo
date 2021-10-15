use std::error::Error as StdError;
use std::future::Future;
use std::net::SocketAddr;
#[cfg(feature = "tls")]
use std::path::Path;
use std::sync::Arc;

use crate::http::Mime;
use futures::{TryStream, TryStreamExt};
use hyper::server::accept::{self, Accept};
use hyper::server::conn::AddrIncoming;
use hyper::Server as HyperServer;
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(feature = "tls")]
use crate::tls::{TlsAcceptor, TlsConfigBuilder};
use crate::transport::LiftIo;
use crate::{Catcher, Router, Service};

pub fn builder<I>(incoming: I) -> hyper::server::Builder<I> {
    HyperServer::builder(incoming)
}

pub struct Server {
    service: Service,
}

impl Server {
    /// Create new Server with router.
    ///
    pub fn new<T>(router: T) -> Server
    where
        T: Into<Arc<Router>>,
    {
        Server {
            service: Service::new(router),
        }
    }

    /// Set custom catchers for server.
    pub fn with_catchers<T>(mut self, catchers: T) -> Self
    where
        T: Into<Arc<Vec<Box<dyn Catcher>>>>,
    {
        self.service.catchers = catchers.into();
        self
    }

    /// Set allowed media types for server, any media type is not include in this list
    /// will not allowed to send to client.
    pub fn with_allowed_media_types<T>(mut self, allowed_media_types: T) -> Self
    where
        T: Into<Arc<Vec<Mime>>>,
    {
        self.service.allowed_media_types = allowed_media_types.into();
        self
    }

    fn create_bind_hyper_server(
        self,
        addr: impl Into<SocketAddr>,
    ) -> Result<(SocketAddr, hyper::Server<AddrIncoming, Service>), hyper::Error> {
        let addr = addr.into();
        let mut incoming = AddrIncoming::bind(&addr)?;
        incoming.set_nodelay(true);
        Ok((addr, builder(incoming).serve(self.service)))
    }

    #[inline]
    fn create_bind_incoming_hyper_server<S>(
        self,
        incoming: S,
    ) -> hyper::Server<impl Accept<Conn = LiftIo<S::Ok>, Error = S::Error>, Service>
    where
        S: TryStream + Send,
        S::Ok: AsyncRead + AsyncWrite + Send + 'static + Unpin,
        S::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        builder(accept::from_stream(incoming.map_ok(LiftIo).into_stream())).serve(self.service)
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
        if let Err(err) = srv.await {
            tracing::error!("server error: {}", err);
            Err(err)
        } else {
            tracing::info!("listening with socket addr: {}", addr);
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
    ///         rx.await.ok();
    ///     });
    ///
    ///     // Spawn the server into a runtime
    ///     tokio::task::spawn(server);
    ///
    ///     // Later, start the shutdown...
    ///     let _ = tx.send(());
    /// }
    /// ```
    pub async fn bind_with_graceful_shutdown(
        self,
        addr: impl Into<SocketAddr> + 'static,
        signal: impl Future<Output = ()> + Send + 'static,
    ) {
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
        let srv = self.create_bind_incoming_hyper_server(incoming);
        tracing::info!("listening with custom incoming");
        if let Err(err) = srv.await {
            tracing::error!("server error: {}", err);
            Err(err)
        } else {
            Ok(())
        }
    }

    pub async fn bind_incoming_with_graceful_shutdown<I>(
        self,
        incoming: I,
        signal: impl Future<Output = ()> + Send + 'static,
    ) where
        I: TryStream + Send,
        I::Ok: AsyncRead + AsyncWrite + Send + 'static + Unpin,
        I::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        self.try_bind_incoming_with_graceful_shutdown(incoming, signal)
            .await
            .unwrap();
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
        let srv = self.create_bind_incoming_hyper_server(incoming);
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
            service: self.service,
            config: TlsConfigBuilder::new(),
        }
    }
}

#[cfg(feature = "tls")]
pub struct TlsServer {
    service: Service,
    config: TlsConfigBuilder,
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
        let TlsServer { service, config } = self;
        let config = func(config);
        TlsServer { service, config }
    }

    #[inline]
    fn create_bind_hyper_server(
        self,
        addr: impl Into<SocketAddr>,
    ) -> Result<(SocketAddr, hyper::Server<TlsAcceptor, Service>), crate::Error> {
        let addr = addr.into();
        let TlsServer { service, config } = self;
        let tls = config.build().map_err(crate::Error::new)?;
        let mut incoming = AddrIncoming::bind(&addr).map_err(crate::Error::new)?;
        incoming.set_nodelay(true);
        let srv = builder(TlsAcceptor::new(tls, incoming)).serve(service);
        Ok((addr, srv))
    }

    pub fn start(self, addr: impl Into<SocketAddr> + 'static) {
        self.start_with_threads(addr, num_cpus::get())
    }

    pub fn start_with_threads(self, addr: impl Into<SocketAddr> + 'static, threads: usize) {
        let runtime = crate::new_runtime(threads);
        let _ = runtime.block_on(async { self.bind(addr).await });
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

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    #[tokio::test]
    async fn test_hello_word() {
        #[fn_handler]
        async fn hello_world() -> Result<&'static str, ()> {
            Ok("Hello World")
        }
        let router = Router::new().get(hello_world);

        tokio::task::spawn(async {
            Server::new(router).bind(([0, 0, 0, 0], 7979)).await;
        });

        let client = reqwest::Client::new();
        let result = client
            .get("http://127.0.0.1:7979")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(result, "Hello World");
        let result = client
            .get("http://127.0.0.1:7979/not_exist")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(result.contains("Not Found"));
    }
}
