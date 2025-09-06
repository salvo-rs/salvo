//! Server module
use std::fmt::{self, Debug, Formatter};
use std::io::Result as IoResult;
use std::sync::Arc;
#[cfg(feature = "server-handle")]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(any(feature = "http1", feature = "http2", feature = "quinn")))]
compile_error!(
    "You have enabled `server` feature, it requires at least one of the following features: http1, http2, quinn."
);

#[cfg(feature = "http1")]
use hyper::server::conn::http1;
#[cfg(feature = "http2")]
use hyper::server::conn::http2;
#[cfg(feature = "server-handle")]
use tokio::{
    sync::{
        Notify,
        mpsc::{UnboundedReceiver, UnboundedSender},
    },
    time::Duration,
};
#[cfg(feature = "server-handle")]
use tokio_util::sync::CancellationToken;

use crate::Service;
#[cfg(feature = "quinn")]
use crate::conn::quinn;
use crate::conn::{Accepted, Coupler, Acceptor, Holding, HttpBuilder};
use crate::fuse::{ArcFuseFactory, FuseFactory};
use crate::http::{HeaderValue,  Version};

cfg_feature! {
    #![feature ="server-handle"]
    /// Server handle is used to stop server.
    #[derive(Clone)]
    pub struct ServerHandle {
        tx_cmd: UnboundedSender<ServerCommand>,
    }
    impl Debug for ServerHandle {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.debug_struct("ServerHandle").finish()
        }
    }
}

#[cfg(feature = "server-handle")]
impl ServerHandle {
    /// Force stop server.
    ///
    /// Call this function will stop server immediately.
    pub fn stop_forcible(&self) {
        let _ = self.tx_cmd.send(ServerCommand::StopForcible);
    }

    /// Graceful stop server.
    ///
    /// Call this function will stop server after all connections are closed,
    /// allowing it to finish processing any ongoing requests before terminating.
    /// It ensures that all connections are closed properly and any resources are released.
    ///
    /// You can specify a timeout to force stop server.
    /// If `timeout` is `None`, it will wait until all connections are closed.
    ///
    /// This function gracefully stop the server, allowing it to finish processing any
    /// ongoing requests before terminating. It ensures that all connections are closed
    /// properly and any resources are released.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use salvo_core::prelude::*;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let acceptor = TcpListener::new("127.0.0.1:8698").bind().await;
    ///     let server = Server::new(acceptor);
    ///     let handle = server.handle();
    ///
    ///     // Graceful shutdown the server
    ///       tokio::spawn(async move {
    ///         tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    ///         handle.stop_graceful(None);
    ///     });
    ///     server.serve(Router::new()).await;
    /// }
    /// ```
    pub fn stop_graceful(&self, timeout: impl Into<Option<Duration>>) {
        let _ = self
            .tx_cmd
            .send(ServerCommand::StopGraceful(timeout.into()));
    }
}

#[cfg(feature = "server-handle")]
enum ServerCommand {
    StopForcible,
    StopGraceful(Option<Duration>),
}

/// HTTP Server.
///
/// A `Server` is created to listen on a port, parse HTTP requests, and hand them off to a [`Service`].
pub struct Server<A> {
    acceptor: A,
    builder: HttpBuilder,
    fuse_factory: Option<ArcFuseFactory>,
    #[cfg(feature = "server-handle")]
    tx_cmd: UnboundedSender<ServerCommand>,
    #[cfg(feature = "server-handle")]
    rx_cmd: UnboundedReceiver<ServerCommand>,
}

impl<A> Debug for Server<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server").finish()
    }
}

impl<A: Acceptor + Send> Server<A> {
    /// Create new `Server` with [`Acceptor`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use salvo_core::prelude::*;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let acceptor = TcpListener::new("127.0.0.1:8698").bind().await;
    ///     Server::new(acceptor);
    /// }
    /// ```
    pub fn new(acceptor: A) -> Self {
        Self::with_http_builder(acceptor, HttpBuilder::new())
    }

    /// Create new `Server` with [`Acceptor`] and [`HttpBuilder`].
    pub fn with_http_builder(acceptor: A, builder: HttpBuilder) -> Self {
        #[cfg(feature = "server-handle")]
        let (tx_cmd, rx_cmd) = tokio::sync::mpsc::unbounded_channel();
        Self {
            acceptor,
            builder,
            fuse_factory: None,
            #[cfg(feature = "server-handle")]
            tx_cmd,
            #[cfg(feature = "server-handle")]
            rx_cmd,
        }
    }

    /// Set the fuse factory.
    #[must_use]
    pub fn fuse_factory<F>(mut self, factory: F) -> Self
    where
        F: FuseFactory + Send + Sync + 'static,
    {
        self.fuse_factory = Some(Arc::new(factory));
        self
    }

    cfg_feature! {
        #![feature = "server-handle"]
        /// Get a [`ServerHandle`] to stop server.
        pub fn handle(&self) -> ServerHandle {
            ServerHandle {
                tx_cmd: self.tx_cmd.clone(),
            }
        }

        /// Force stop server.
        ///
        /// Call this function will stop server immediately.
        pub fn stop_forcible(&self) {
            let _ = self.tx_cmd.send(ServerCommand::StopForcible);
        }

        /// Graceful stop server.
        ///
        /// Call this function will stop server after all connections are closed.
        /// You can specify a timeout to force stop server.
        /// If `timeout` is `None`, it will wait until all connections are closed.
        pub fn stop_graceful(&self, timeout: impl Into<Option<Duration>>) {
            let _ = self.tx_cmd.send(ServerCommand::StopGraceful(timeout.into()));
        }
    }

    /// Get holding information of this server.
    #[inline]
    pub fn holdings(&self) -> &[Holding] {
        self.acceptor.holdings()
    }

    cfg_feature! {
        #![feature = "http1"]
        /// Use this function to set http1 protocol.
        pub fn http1_mut(&mut self) -> &mut http1::Builder {
            &mut self.builder.http1
        }
    }

    cfg_feature! {
        #![feature = "http2"]
        /// Use this function to set http2 protocol.
        pub fn http2_mut(&mut self) -> &mut http2::Builder<crate::rt::tokio::TokioExecutor> {
            &mut self.builder.http2
        }
    }

    cfg_feature! {
        #![feature = "quinn"]
        /// Use this function to set http3 protocol.
        pub fn quinn_mut(&mut self) -> &mut quinn::Builder {
            &mut self.builder.quinn
        }
    }

    /// Serve a [`Service`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use salvo_core::prelude::*;
    /// #[handler]
    /// async fn hello() -> &'static str {
    ///     "Hello World"
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    ///     let router = Router::new().get(hello);
    ///     Server::new(acceptor).serve(router).await;
    /// }
    /// ```
    #[inline]
    pub async fn serve<S>(self, service: S)
    where
        S: Into<Service> + Send,
    {
        self.try_serve(service)
            .await
            .expect("failed to call `Server::serve`");
    }

    /// Try to serve a [`Service`].
    #[cfg(feature = "server-handle")]
    #[allow(clippy::manual_async_fn)] //Fix: https://github.com/salvo-rs/salvo/issues/902
    pub fn try_serve<S>(self, service: S) -> impl Future<Output = IoResult<()>> + Send
    where
        S: Into<Service> + Send,
    {
        async {
            let Self {
                mut acceptor,
                builder,
                fuse_factory,
                mut rx_cmd,
                ..
            } = self;
            let alive_connections = Arc::new(AtomicUsize::new(0));
            let notify = Arc::new(Notify::new());
            let force_stop_token = CancellationToken::new();
            let graceful_stop_token = CancellationToken::new();

            let mut alt_svc_h3 = None;
            for holding in acceptor.holdings() {
                tracing::info!("listening {}", holding);
                if holding.http_versions.contains(&Version::HTTP_3) {
                    if let Some(addr) = holding.local_addr.clone().into_std() {
                        let port = addr.port();
                        alt_svc_h3 = Some(
                            format!(r#"h3=":{port}"; ma=2592000,h3-29=":{port}"; ma=2592000"#)
                                .parse::<HeaderValue>()
                                .expect("Parse alt-svc header should not failed."),
                        );
                    }
                }
            }

            let service: Arc<Service> = Arc::new(service.into());
            let builder = Arc::new(builder);
            loop {
                tokio::select! {
                    accepted = acceptor.accept(fuse_factory.clone()) => {
                        match accepted {
                            Ok(Accepted { coupler, stream, fusewire, local_addr, remote_addr, http_scheme, ..}) => {
                                alive_connections.fetch_add(1, Ordering::Release);

                                let service = service.clone();
                                let alive_connections = alive_connections.clone();
                                let notify = notify.clone();
                                let handler = service.hyper_handler(local_addr, remote_addr, http_scheme, fusewire, alt_svc_h3.clone());
                                let builder = builder.clone();

                                let force_stop_token = force_stop_token.clone();
                                let graceful_stop_token = graceful_stop_token.clone();

                                tokio::spawn(async move {
                                    let conn = coupler.couple(stream, handler, builder, Some(graceful_stop_token.clone()));
                                    tokio::select! {
                                        _ = conn => {
                                        },
                                        _ = force_stop_token.cancelled() => {
                                        }
                                    }

                                    if alive_connections.fetch_sub(1, Ordering::Acquire) == 1 {
                                        // notify only if shutdown is initiated, to prevent notification when server is active.
                                        // It's a valid state to have 0 alive connections when server is not shutting down.
                                        if graceful_stop_token.is_cancelled() {
                                            notify.notify_one();
                                        }
                                    }
                                });
                            },
                            Err(e) => {
                                tracing::error!(error = ?e, "accept connection failed");
                            }
                        }
                    }
                    Some(cmd) = rx_cmd.recv() => {
                        match cmd {
                            ServerCommand::StopGraceful(timeout) => {
                                let graceful_stop_token = graceful_stop_token.clone();
                                graceful_stop_token.cancel();
                                if let Some(timeout) = timeout {
                                    tracing::info!(
                                        timeout_in_seconds = timeout.as_secs_f32(),
                                        "initiate graceful stop server",
                                    );

                                    let force_stop_token = force_stop_token.clone();
                                    tokio::spawn(async move {
                                        tokio::time::sleep(timeout).await;
                                        force_stop_token.cancel();
                                    });
                                } else {
                                    tracing::info!("initiate graceful stop server");
                                }
                            },
                            ServerCommand::StopForcible => {
                                tracing::info!("force stop server");
                                force_stop_token.cancel();
                            },
                        }
                        break;
                    },
                }
            }

            if !force_stop_token.is_cancelled() && alive_connections.load(Ordering::Acquire) > 0 {
                tracing::info!(
                    "wait for {} connections to close.",
                    alive_connections.load(Ordering::Acquire)
                );
                notify.notified().await;
            }

            tracing::info!("server stopped");
            Ok(())
        }
    }
    /// Try to serve a [`Service`].
    #[cfg(not(feature = "server-handle"))]
    pub async fn try_serve<S>(self, service: S) -> IoResult<()>
    where
        S: Into<Service> + Send,
    {
        let Self {
            mut acceptor,
            builder,
            fuse_factory,
            ..
        } = self;
        let mut alt_svc_h3 = None;
        for holding in acceptor.holdings() {
            tracing::info!("listening {}", holding);
            if holding.http_versions.contains(&Version::HTTP_3) {
                if let Some(addr) = holding.local_addr.clone().into_std() {
                    let port = addr.port();
                    alt_svc_h3 = Some(
                        format!(r#"h3=":{port}"; ma=2592000,h3-29=":{port}"; ma=2592000"#)
                            .parse::<HeaderValue>()
                            .expect("Parse alt-svc header should not failed."),
                    );
                }
            }
        }

        let service: Arc<Service> = Arc::new(service.into());
        let builder = Arc::new(builder);
        loop {
            match acceptor.accept(fuse_factory.clone()).await {
                Ok(Accepted {
                    coupler,
                    stream,
                    fusewire,
                    local_addr,
                    remote_addr,
                    http_scheme,
                    ..
                }) => {
                    let service = service.clone();
                    let handler = service.hyper_handler(
                        local_addr,
                        remote_addr,
                        http_scheme,
                        fusewire,
                        alt_svc_h3.clone(),
                    );
                    let builder = builder.clone();

                    tokio::spawn(async move {
                        let _ = coupler.couple(stream, handler, builder, None).await;
                    });
                }
                Err(e) => {
                    tracing::error!(error = ?e, "accept connection failed");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use std::future::Future;

    use crate::prelude::*;
    use crate::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_server() {
        #[handler]
        async fn hello() -> Result<&'static str, ()> {
            Ok("Hello World")
        }
        #[handler]
        async fn json(res: &mut Response) {
            #[derive(Serialize, Debug)]
            struct User {
                name: String,
            }
            res.render(Json(User {
                name: "jobs".into(),
            }));
        }
        let router = Router::new()
            .get(hello)
            .push(Router::with_path("json").get(json));
        let service = Service::new(router);

        let base_url = "http://127.0.0.1:8698";
        let result = TestClient::get(base_url)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(result, "Hello World");

        let result = TestClient::get(format!("{base_url}/json"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(result, r#"{"name":"jobs"}"#);

        let result = TestClient::get(format!("{base_url}/not_exist"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(result.contains("Not Found"));
        let result = TestClient::get(format!("{base_url}/not_exist"))
            .add_header("accept", "application/json", true)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(result.contains(r#""code":404"#));
        let result = TestClient::get(format!("{base_url}/not_exist"))
            .add_header("accept", "text/plain", true)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(result.contains("code: 404"));
        let result = TestClient::get(format!("{base_url}/not_exist"))
            .add_header("accept", "application/xml", true)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(result.contains("<code>404</code>"));
    }

    #[cfg(feature = "server-handle")]
    #[tokio::test]
    async fn test_server_handle_stop() {
        use std::time::Duration;
        use tokio::time::timeout;

        // Test forcible stop
        let acceptor = crate::conn::TcpListener::new("127.0.0.1:5802").bind().await;
        let server = Server::new(acceptor);
        let handle = server.handle();
        let server_task = tokio::spawn(server.try_serve(Router::new()));

        // Give server a moment to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        handle.stop_forcible();

        let result = timeout(Duration::from_secs(1), server_task).await;
        assert!(result.is_ok(), "Server should stop forcibly within 1 second.");
        let server_result = result.unwrap();
        assert!(server_result.is_ok(), "Server task should not panic.");
        assert!(server_result.unwrap().is_ok(), "try_serve should return Ok.");

        // Test graceful stop
        let acceptor = crate::conn::TcpListener::new("127.0.0.1:5803").bind().await;
        let server = Server::new(acceptor);
        let handle = server.handle();
        let server_task = tokio::spawn(server.try_serve(Router::new()));

        // Give server a moment to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        handle.stop_graceful(None);

        let result = timeout(Duration::from_secs(1), server_task).await;
        assert!(result.is_ok(), "Server should stop gracefully within 1 second.");
        let server_result = result.unwrap();
        assert!(server_result.is_ok(), "Server task should not panic.");
        assert!(server_result.unwrap().is_ok(), "try_serve should return Ok.");
    }

    #[test]
    fn test_regression_209() {
        #[cfg(feature = "acme")]
        let _: &dyn Send = &async {
            let acceptor = TcpListener::new("127.0.0.1:0")
                .acme()
                .add_domain("test.salvo.rs")
                .bind()
                .await;
            Server::new(acceptor).serve(Router::new()).await;
        };
        #[cfg(feature = "native-tls")]
        let _: &dyn Send = &async {
            use crate::conn::native_tls::NativeTlsConfig;

            let identity = if cfg!(target_os = "macos") {
                include_bytes!("../certs/identity-legacy.p12").to_vec()
            } else {
                include_bytes!("../certs/identity.p12").to_vec()
            };
            let acceptor = TcpListener::new("127.0.0.1:0")
                .native_tls(NativeTlsConfig::new().pkcs12(identity).password("mypass"))
                .bind()
                .await;
            Server::new(acceptor).serve(Router::new()).await;
        };
        #[cfg(feature = "openssl")]
        let _: &dyn Send = &async {
            use crate::conn::openssl::{Keycert, OpensslConfig};

            let acceptor = TcpListener::new("127.0.0.1:0")
                .openssl(OpensslConfig::new(
                    Keycert::new()
                        .key_from_path("certs/key.pem")
                        .unwrap()
                        .cert_from_path("certs/cert.pem")
                        .unwrap(),
                ))
                .bind()
                .await;
            Server::new(acceptor).serve(Router::new()).await;
        };
        #[cfg(feature = "rustls")]
        let _: &dyn Send = &async {
            use crate::conn::rustls::{Keycert, RustlsConfig};

            let acceptor = TcpListener::new("127.0.0.1:0")
                .rustls(RustlsConfig::new(
                    Keycert::new()
                        .key_from_path("certs/key.pem")
                        .unwrap()
                        .cert_from_path("certs/cert.pem")
                        .unwrap(),
                ))
                .bind()
                .await;
            Server::new(acceptor).serve(Router::new()).await;
        };
        #[cfg(feature = "quinn")]
        let _: &dyn Send = &async {
            use crate::conn::rustls::{Keycert, RustlsConfig};

            let cert = include_bytes!("../certs/cert.pem").to_vec();
            let key = include_bytes!("../certs/key.pem").to_vec();
            let config =
                RustlsConfig::new(Keycert::new().cert(cert.as_slice()).key(key.as_slice()));
            let listener = TcpListener::new(("127.0.0.1", 2048)).rustls(config.clone());
            let acceptor = QuinnListener::new(config, ("127.0.0.1", 2048))
                .join(listener)
                .bind()
                .await;
            Server::new(acceptor).serve(Router::new()).await;
        };
        let _: &dyn Send = &async {
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 6878));
            let acceptor = TcpListener::new(addr).bind().await;
            Server::new(acceptor).serve(Router::new()).await;
        };
        #[cfg(unix)]
        let _: &dyn Send = &async {
            use crate::conn::UnixListener;

            let sock_file = "/tmp/test-salvo.sock";
            let acceptor = UnixListener::new(sock_file).bind().await;
            Server::new(acceptor).serve(Router::new()).await;
        };
    }
}
