//! Server module
use std::future::Future;
use std::io::Result as IoResult;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[cfg(feature = "http1")]
use hyper::server::conn::http1;
#[cfg(feature = "http2")]
use hyper::server::conn::http2;
use tokio::sync::Notify;
use tokio::time::Duration;

#[cfg(feature = "quinn")]
use crate::conn::quinn;
use crate::conn::{Accepted, Acceptor, Holding, HttpBuilder};
use crate::http::{HeaderValue, HttpConnection, Version};
use crate::Service;

/// HTTP Server
///
/// A `Server` is created to listen on a port, parse HTTP requests, and hand them off to a [`Service`].
pub struct Server<A> {
    acceptor: A,
    builder: HttpBuilder,
}

impl<A: Acceptor + Send> Server<A> {
    /// Create new `Server` with [`Acceptor`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use salvo_core::prelude::*;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    /// Server::new(acceptor);
    /// # }
    /// ```
    #[inline]
    pub fn new(acceptor: A) -> Self {
        Server {
            acceptor,
            builder: HttpBuilder {
                #[cfg(feature = "http1")]
                http1: http1::Builder::new(),
                #[cfg(feature = "http2")]
                http2: http2::Builder::new(crate::rt::TokioExecutor::new()),
                #[cfg(feature = "quinn")]
                quinn: crate::conn::quinn::Builder::new(),
            },
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
        pub fn http2_mut(&mut self) -> &mut http2::Builder<crate::rt::TokioExecutor> {
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

    /// Serve a [`Service`]
    #[inline]
    pub async fn serve<S>(self, service: S)
    where
        S: Into<Service> + Send,
    {
        self.try_serve(service).await.unwrap();
    }

    /// Try to serve a [`Service`]
    #[inline]
    pub async fn try_serve<S>(self, service: S) -> IoResult<()>
    where
        S: Into<Service> + Send,
    {
        self.try_serve_with_graceful_shutdown(service, futures_util::future::pending(), None)
            .await
    }

    /// Serve with graceful shutdown signal.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use tokio::sync::oneshot;
    ///
    /// use salvo_core::prelude::*;
    ///
    /// #[handler]
    /// async fn hello(res: &mut Response) {
    ///     res.render("Hello World!");
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, rx) = oneshot::channel();
    ///     let router = Router::new().get(hello);
    ///     let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    ///     let server = Server::new(acceptor).serve_with_graceful_shutdown(router, async {
    ///         rx.await.ok();
    ///     }, None);
    ///
    ///     // Spawn the server into a runtime
    ///     tokio::task::spawn(server);
    ///
    ///     // Later, start the shutdown...
    ///     let _ = tx.send(());
    /// }
    /// ```
    #[inline]
    pub async fn serve_with_graceful_shutdown<S, G>(self, service: S, signal: G, timeout: Option<Duration>)
    where
        S: Into<Service> + Send,
        G: Future<Output = ()> + Send + 'static,
    {
        self.try_serve_with_graceful_shutdown(service, signal, timeout)
            .await
            .unwrap();
    }

    /// Serve with graceful shutdown signal.
    #[inline]
    pub async fn try_serve_with_graceful_shutdown<S, G>(
        self,
        service: S,
        signal: G,
        timeout: Option<Duration>,
    ) -> IoResult<()>
    where
        S: Into<Service> + Send,
        G: Future<Output = ()> + Send + 'static,
    {
        let Self { mut acceptor, builder } = self;
        let alive_connections = Arc::new(AtomicUsize::new(0));
        let notify = Arc::new(Notify::new());
        let timeout_notify = Arc::new(Notify::new());

        tokio::pin!(signal);

        let mut alt_svc_h3 = None;
        for holding in acceptor.holdings() {
            tracing::info!("listening {}", holding);
            if holding.http_versions.contains(&Version::HTTP_3) {
                if let Some(addr) = holding.local_addr.clone().into_std() {
                    let port = addr.port();
                    alt_svc_h3 = Some(
                        format!(r#"h3-29=":{port}"; ma=2592000,quic=":{port}"; ma=2592000; v="46,43""#)
                            .parse::<HeaderValue>()
                            .unwrap(),
                    );
                }
            }
        }

        let service = Arc::new(service.into());
        let builder = Arc::new(builder);
        loop {
            tokio::select! {
                _ = &mut signal => {
                    if let Some(timeout) = timeout {
                        tracing::info!(
                            timeout_in_seconds = timeout.as_secs_f32(),
                            "initiate graceful shutdown",
                        );

                        let timeout_notify = timeout_notify.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(timeout).await;
                            timeout_notify.notify_waiters();
                        });
                    } else {
                        tracing::info!("initiate graceful shutdown");
                    }
                    break;
                },
                 accepted = acceptor.accept() => {
                    match accepted {
                        Ok(Accepted { conn, local_addr, remote_addr, http_scheme, ..}) => {
                            let service = service.clone();
                            let alive_connections = alive_connections.clone();
                            let notify = notify.clone();
                            let timeout_notify = timeout_notify.clone();
                            let handler = service.hyper_handler(local_addr, remote_addr, http_scheme, alt_svc_h3.clone());
                            let builder = builder.clone();
                            tokio::spawn(async move {
                                alive_connections.fetch_add(1, Ordering::SeqCst);
                                let conn = conn.serve(handler, builder);
                                if timeout.is_some() {
                                    tokio::select! {
                                        _ = conn => {},
                                        _ = timeout_notify.notified() => {}
                                    }
                                } else {
                                    conn.await.ok();
                                }

                                if alive_connections.fetch_sub(1, Ordering::SeqCst) == 1 {
                                    notify.notify_one();
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

        if alive_connections.load(Ordering::SeqCst) > 0 {
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
        let result = TestClient::get(&base_url)
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
