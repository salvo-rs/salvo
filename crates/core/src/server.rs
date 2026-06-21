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

use tokio::sync::Semaphore;

use crate::Service;
#[cfg(feature = "quinn")]
use crate::conn::quinn;
use crate::conn::{Accepted, Acceptor, Coupler, Holding, HttpBuilder};
use crate::fuse::{ArcFuseFactory, FuseFactory};

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
    /// Forcefully stops the server immediately, without waiting for in-flight
    /// requests to finish. The adjective parallels [`Self::stop_graceful`];
    /// reach for that one when you want a clean shutdown.
    pub fn stop_forceful(&self) {
        let _ = self.tx_cmd.send(ServerCommand::StopForceful);
    }

    /// Deprecated alias for [`Self::stop_forceful`].
    ///
    /// The old name reads awkwardly (`forcible` is an adjective for things
    /// that *can* be forced, not for the act of forcing, and it does not
    /// parallel the adjective in `stop_graceful`). Use [`stop_forceful`] for
    /// new code; this shim keeps existing callers compiling.
    ///
    /// [`stop_forceful`]: Self::stop_forceful
    #[deprecated(since = "0.94.0", note = "use `ServerHandle::stop_forceful` instead")]
    pub fn stop_forcible(&self) {
        self.stop_forceful();
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
    StopForceful,
    StopGraceful(Option<Duration>),
}

/// HTTP Server.
///
/// A `Server` is created to listen on a port, parse HTTP requests, and hand them off to a [`Service`].
pub struct Server<A> {
    acceptor: A,
    builder: HttpBuilder,
    fuse_factory: Option<ArcFuseFactory>,
    /// Maximum number of concurrent connections; `None` means unlimited.
    max_connections: Option<usize>,
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
    /// Creates a new `Server` with an [`Acceptor`].
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

    /// Creates a new `Server` with an [`Acceptor`] and [`HttpBuilder`].
    pub fn with_http_builder(acceptor: A, builder: HttpBuilder) -> Self {
        #[cfg(feature = "server-handle")]
        let (tx_cmd, rx_cmd) = tokio::sync::mpsc::unbounded_channel();
        Self {
            acceptor,
            builder,
            fuse_factory: None,
            max_connections: None,
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

    /// Limit the number of concurrent connections the server will handle.
    ///
    /// Once `max` connections are active, the accept loop stops accepting new
    /// connections (applying backpressure to the OS listen backlog) until an
    /// existing connection closes. This bounds memory and file-descriptor use
    /// under load or connection-exhaustion attacks. By default there is no limit.
    #[must_use]
    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = Some(max);
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

        /// Forcefully stops the server immediately, without waiting for in-flight
        /// requests to finish. The adjective parallels [`Self::stop_graceful`];
        /// reach for that one when you want a clean shutdown.
        pub fn stop_forceful(&self) {
            let _ = self.tx_cmd.send(ServerCommand::StopForceful);
        }

        /// Deprecated alias for [`Self::stop_forceful`].
        #[deprecated(since = "0.94.0", note = "use `Server::stop_forceful` instead")]
        pub fn stop_forcible(&self) {
            self.stop_forceful();
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
        /// Use this function to set the HTTP/3 protocol.
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
        async move {
            let Self {
                mut acceptor,
                builder,
                fuse_factory,
                max_connections,
                mut rx_cmd,
                ..
            } = self;
            let alive_connections = Arc::new(AtomicUsize::new(0));
            let notify = Arc::new(Notify::new());
            let force_stop_token = CancellationToken::new();
            let graceful_stop_token = CancellationToken::new();
            let conn_semaphore = max_connections.map(|max| Arc::new(Semaphore::new(max)));

            #[cfg(not(feature = "quinn"))]
            let alt_svc_h3 = None;
            #[cfg(feature = "quinn")]
            let mut alt_svc_h3 = None;

            for holding in acceptor.holdings() {
                tracing::info!("listening {}", holding);

                #[cfg(feature = "quinn")]
                {
                    use crate::http::{HeaderValue, Version};

                    if builder.quinn.auto_alt_svc_header
                        && holding.http_versions.contains(&Version::HTTP_3)
                        && let Some(addr) = holding.local_addr.clone().into_std()
                    {
                        let port = addr.port();
                        alt_svc_h3 = Some(
                            format!(r#"h3=":{port}"; ma=2592000,h3-29=":{port}"; ma=2592000"#)
                                .parse::<HeaderValue>()
                                .expect("parsing alt-svc header should not fail"),
                        );
                    }
                }
            }

            let service: Arc<Service> = Arc::new(service.into());
            let builder = Arc::new(builder);
            // Apply a received stop command. Used from both the permit-acquire and the
            // accept `select!` so shutdown is observed even while saturated.
            macro_rules! handle_stop_command {
                ($cmd:expr) => {
                    match $cmd {
                        ServerCommand::StopGraceful(timeout) => {
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
                        }
                        ServerCommand::StopForceful => {
                            tracing::info!("force stop server");
                            force_stop_token.cancel();
                        }
                    }
                };
            }
            loop {
                // Acquire a connection permit before accepting (backpressure when at
                // `max_connections`). Race it against the stop channel so a *saturated*
                // server (no free permit) still observes shutdown instead of blocking
                // the accept loop forever in `acquire_owned`.
                let permit = if let Some(semaphore) = &conn_semaphore {
                    tokio::select! {
                        biased;
                        Some(cmd) = rx_cmd.recv() => {
                            handle_stop_command!(cmd);
                            break;
                        }
                        permit = semaphore.clone().acquire_owned() => {
                            Some(permit.expect("connection semaphore is never closed"))
                        }
                    }
                } else {
                    None
                };
                tokio::select! {
                    accepted = acceptor.accept(fuse_factory.clone()) => {
                        match accepted {
                            Ok(Accepted { coupler, stream, fusewire, local_addr, remote_addr, http_scheme}) => {
                                alive_connections.fetch_add(1, Ordering::Release);

                                let service = service.clone();
                                let alive_connections = alive_connections.clone();
                                let notify = notify.clone();
                                let handler = service.hyper_handler(local_addr, remote_addr, http_scheme, fusewire, alt_svc_h3.clone());
                                let builder = builder.clone();

                                let force_stop_token = force_stop_token.clone();
                                let graceful_stop_token = graceful_stop_token.clone();

                                tokio::spawn(async move {
                                    // Hold the permit for the connection's lifetime; it is
                                    // released back to the semaphore when this task ends.
                                    let _permit = permit;
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
                                // Back off briefly so a persistent accept error
                                // (e.g. `EMFILE` when out of file descriptors) does
                                // not spin this loop at 100% CPU and flood the logs.
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        }
                    }
                    Some(cmd) = rx_cmd.recv() => {
                        handle_stop_command!(cmd);
                        break;
                    },
                }
            }

            if !force_stop_token.is_cancelled() && alive_connections.load(Ordering::Acquire) > 0 {
                tracing::info!(
                    "wait for {} connections to close.",
                    alive_connections.load(Ordering::Acquire)
                );
                // Re-check in a loop and also wake on a force-stop: a connection that
                // ignores `force_stop_token` would otherwise never decrement the count,
                // so a single `notify.notified().await` could hang shutdown forever.
                while !force_stop_token.is_cancelled()
                    && alive_connections.load(Ordering::Acquire) > 0
                {
                    tokio::select! {
                        _ = notify.notified() => {}
                        _ = force_stop_token.cancelled() => break,
                    }
                }
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
            max_connections,
            ..
        } = self;
        let conn_semaphore = max_connections.map(|max| Arc::new(Semaphore::new(max)));

        #[cfg(not(feature = "quinn"))]
        let alt_svc_h3 = None;
        #[cfg(feature = "quinn")]
        let mut alt_svc_h3 = None;

        for holding in acceptor.holdings() {
            tracing::info!("listening {}", holding);

            #[cfg(feature = "quinn")]
            {
                use crate::http::{HeaderValue, Version};

                if builder.quinn.auto_alt_svc_header
                    && holding.http_versions.contains(&Version::HTTP_3)
                    && let Some(addr) = holding.local_addr.clone().into_std()
                {
                    let port = addr.port();
                    alt_svc_h3 = Some(
                        format!(r#"h3=":{port}"; ma=2592000,h3-29=":{port}"; ma=2592000"#)
                            .parse::<HeaderValue>()
                            .expect("parsing alt-svc header should not fail"),
                    );
                }
            }
        }

        let service: Arc<Service> = Arc::new(service.into());
        let builder = Arc::new(builder);
        loop {
            // Acquire a connection permit before accepting (backpressure when at
            // `max_connections`). There is no graceful-stop channel in this build,
            // so a plain blocking acquire is sufficient.
            let permit = match &conn_semaphore {
                Some(semaphore) => Some(
                    semaphore
                        .clone()
                        .acquire_owned()
                        .await
                        .expect("connection semaphore is never closed"),
                ),
                None => None,
            };
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
                        let _permit = permit;
                        let _ = coupler.couple(stream, handler, builder, None).await;
                    });
                }
                Err(e) => {
                    tracing::error!(error = ?e, "accept connection failed");
                    // Back off briefly so a persistent accept error (e.g. `EMFILE`
                    // when the process is out of file descriptors) does not spin
                    // this loop at 100% CPU and flood the logs.
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

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

        handle.stop_forceful();

        let result = timeout(Duration::from_secs(1), server_task).await;
        assert!(
            result.is_ok(),
            "Server should stop forcibly within 1 second."
        );
        let server_result = result.unwrap();
        assert!(server_result.is_ok(), "Server task should not panic.");
        assert!(
            server_result.unwrap().is_ok(),
            "try_serve should return Ok."
        );

        // Test graceful stop
        let acceptor = crate::conn::TcpListener::new("127.0.0.1:5803").bind().await;
        let server = Server::new(acceptor);
        let handle = server.handle();
        let server_task = tokio::spawn(server.try_serve(Router::new()));

        // Give server a moment to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        handle.stop_graceful(None);

        let result = timeout(Duration::from_secs(1), server_task).await;
        assert!(
            result.is_ok(),
            "Server should stop gracefully within 1 second."
        );
        let server_result = result.unwrap();
        assert!(server_result.is_ok(), "Server task should not panic.");
        assert!(
            server_result.unwrap().is_ok(),
            "try_serve should return Ok."
        );
    }

    #[cfg(feature = "server-handle")]
    #[tokio::test]
    async fn test_server_max_connections_stops_while_saturated() {
        use std::time::Duration;

        use tokio::io::AsyncWriteExt;
        use tokio::time::timeout;

        use crate::conn::Acceptor;

        // A long-lived handler so the single allowed connection stays open and the
        // accept loop is parked in `acquire_owned` with no free permit.
        #[handler]
        async fn slow() {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }

        let acceptor = crate::conn::TcpListener::new("127.0.0.1:0").bind().await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let server = Server::new(acceptor).max_connections(1);
        let handle = server.handle();
        let server_task = tokio::spawn(server.try_serve(Router::new().goal(slow)));

        // Saturate the single permit: open a connection and send a request that the
        // slow handler keeps busy, so no permit is free.
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Force stop must break the accept loop even though it is saturated.
        handle.stop_forceful();
        let result = timeout(Duration::from_secs(2), server_task).await;
        assert!(
            result.is_ok(),
            "saturated connection-limited server must still force-stop"
        );
        assert!(result.unwrap().unwrap().is_ok(), "try_serve should return Ok");
        drop(stream);
    }

    #[test]
    fn test_regression_209() {
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
        #[cfg(all(feature = "unix", unix))]
        let _: &dyn Send = &async {
            use crate::conn::UnixListener;

            let sock_file = "/tmp/test-salvo.sock";
            let acceptor = UnixListener::new(sock_file).bind().await;
            Server::new(acceptor).serve(Router::new()).await;
        };
    }
}
