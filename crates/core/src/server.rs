//! Server module
use std::io::Result as IoResult;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[cfg(not(any(feature = "http1", feature = "http2", feature = "quinn")))]
compile_error!(
    "You have enabled `server` feature, it requires at least one of the following features: http1, http2, quinn."
);

#[cfg(feature = "http1")]
use hyper::server::conn::http1;
#[cfg(feature = "http2")]
use hyper::server::conn::http2;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::Notify;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "quinn")]
use crate::conn::quinn;
use crate::conn::{Accepted, Acceptor, Holding, HttpBuilder};
use crate::fuse::{ArcFuseFactory, FuseFactory, SteadyFusewire};
use crate::http::{HeaderValue, HttpConnection, Version};
use crate::Service;

/// Server handle is used to stop server.
#[derive(Clone)]
pub struct ServerHandle {
    tx_cmd: UnboundedSender<ServerCommand>,
}

impl ServerHandle {
    /// Force stop server.
    ///
    /// Call this function will stop server immediately.
    pub fn stop_forcible(&self) {
        self.tx_cmd.send(ServerCommand::StopForcible).ok();
    }

    /// Graceful stop server.
    ///
    /// Call this function will stop server after all connections are closed,
    /// allowing it to finish processing any ongoing requests before terminating.
    /// It ensures that all connections are closed properly and any resources are released.
    ///
    /// You can specify a timeout to force stop server.
    /// If `timeout` is `None`, it will wait util all connections are closed.
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
    ///     let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
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
        self.tx_cmd.send(ServerCommand::StopGraceful(timeout.into())).ok();
    }
}

enum ServerCommand {
    StopForcible,
    StopGraceful(Option<Duration>),
}

/// HTTP Server
///
/// A `Server` is created to listen on a port, parse HTTP requests, and hand them off to a [`Service`].
pub struct Server<A> {
    acceptor: A,
    builder: HttpBuilder,
    fuse_factory: ArcFuseFactory,
    tx_cmd: UnboundedSender<ServerCommand>,
    rx_cmd: UnboundedReceiver<ServerCommand>,
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
    ///     let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    ///     Server::new(acceptor);
    /// }
    /// ```
    pub fn new(acceptor: A) -> Self {
        Self::with_http_builder(acceptor, HttpBuilder::new())
    }

    /// Create new `Server` with [`Acceptor`] and [`HttpBuilder`].
    pub fn with_http_builder(acceptor: A, builder: HttpBuilder) -> Self {
        let (tx_cmd, rx_cmd) = tokio::sync::mpsc::unbounded_channel();
        Self {
            acceptor,
            builder,
            fuse_factory: Arc::new(SteadyFusewire),
            tx_cmd,
            rx_cmd,
        }
    }

    /// Set the fuse factory.
    pub fn fuse_factory<F>(mut self, factory: F) -> Self
    where
        F: FuseFactory + Send + Sync + 'static,
    {
        self.fuse_factory = Arc::new(factory);
        self
    }

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
        self.tx_cmd.send(ServerCommand::StopForcible).ok();
    }

    /// Graceful stop server.
    ///
    /// Call this function will stop server after all connections are closed.
    /// You can specify a timeout to force stop server.
    /// If `timeout` is `None`, it will wait util all connections are closed.
    pub fn stop_graceful(&self, timeout: impl Into<Option<Duration>>) {
        self.tx_cmd.send(ServerCommand::StopGraceful(timeout.into())).ok();
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
    ///     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    ///     let router = Router::new().get(hello);
    ///     Server::new(acceptor).serve(router).await;
    /// }
    /// ```
    #[inline]
    pub async fn serve<S>(self, service: S)
    where
        S: Into<Service> + Send,
    {
        self.try_serve(service).await.expect("failed to call `Server::serve`");
    }

    /// Try to serve a [`Service`].
    pub async fn try_serve<S>(self, service: S) -> IoResult<()>
    where
        S: Into<Service> + Send,
    {
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
                            .expect("Parse alt-svc header failed."),
                    );
                }
            }
        }

        let service: Arc<Service> = Arc::new(service.into());
        let builder = Arc::new(builder);
        loop {
            tokio::select! {
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
                accepted = acceptor.accept(fuse_factory.clone()) => {
                    match accepted {
                        Ok(Accepted { conn, local_addr, remote_addr, http_scheme, ..}) => {
                            alive_connections.fetch_add(1, Ordering::Release);

                            let service = service.clone();
                            let alive_connections = alive_connections.clone();
                            let notify = notify.clone();
                            let handler = service.hyper_handler(local_addr, remote_addr, http_scheme, conn.fusewire(), alt_svc_h3.clone());
                            let builder = builder.clone();

                            let force_stop_token = force_stop_token.clone();
                            let graceful_stop_token = graceful_stop_token.clone();

                            tokio::spawn(async move {
                                let conn = conn.serve(handler, builder, graceful_stop_token);
                                tokio::select! {
                                    _ = conn => {
                                    },
                                    _ = force_stop_token.cancelled() => {
                                    }
                                }

                                if alive_connections.fetch_sub(1, Ordering::Acquire) == 1 {
                                    notify.notify_waiters();
                                }
                            });
                        },
                        Err(e) => {
                            tracing::error!(error = ?e, "accept connection failed");
                        }
                    }
                }
            }
        }

        if alive_connections.load(Ordering::Acquire) > 0 {
            tracing::info!("wait for all connections to close.");
            notify.notified().await;
        }

        tracing::info!("server stopped");
        Ok(())
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
            res.render(Json(User { name: "jobs".into() }));
        }
        let router = Router::new().get(hello).push(Router::with_path("json").get(json));
        let serivce = Service::new(router);

        let base_url = "http://127.0.0.1:5800";
        let result = TestClient::get(base_url)
            .send(&serivce)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(result, "Hello World");

        let result = TestClient::get(format!("{}/json", base_url))
            .send(&serivce)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(result, r#"{"name":"jobs"}"#);

        let result = TestClient::get(format!("{}/not_exist", base_url))
            .send(&serivce)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(result.contains("Not Found"));
        let result = TestClient::get(format!("{}/not_exist", base_url))
            .add_header("accept", "application/json", true)
            .send(&serivce)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(result.contains(r#""code":404"#));
        let result = TestClient::get(format!("{}/not_exist", base_url))
            .add_header("accept", "text/plain", true)
            .send(&serivce)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(result.contains("code: 404"));
        let result = TestClient::get(format!("{}/not_exist", base_url))
            .add_header("accept", "application/xml", true)
            .send(&serivce)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(result.contains("<code>404</code>"));
    }
}
