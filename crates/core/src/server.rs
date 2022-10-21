//! Server module
use std::future::Future;
use std::io::Result as IoResult;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[cfg(feature = "http3")]
use h3::error::ErrorLevel;
#[cfg(feature = "http1")]
use hyper::server::conn::http1;
#[cfg(feature = "http2")]
use hyper::server::conn::http2;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Notify;
use tokio::time::Duration;

use crate::conn::{Accepted, Acceptor, Listener};
use crate::http::version::{Version, VersionDetector};
use crate::runtimes::{TokioExecutor, TokioTimer};
use crate::Service;

/// HTTP Server
///
/// A `Server` is created to listen on a port, parse HTTP requests, and hand them off to a [`Service`].
pub struct Server<L> {
    listener: L,
    #[cfg(feature = "http1")]
    http1: Arc<http1::Builder>,
    #[cfg(feature = "http2")]
    http2: Arc<http2::Builder<TokioExecutor>>,
}

impl<L> Server<L>
where
    L: Listener,
    <L::Acceptor as Acceptor>::Conn: VersionDetector,
{
    /// Create new `Server` with [`Listener`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use salvo_core::prelude::*;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// Server::new(TcpListener::bind("127.0.0.1:7878"));
    /// # }
    /// ```
    #[inline]
    pub fn new(listener: L) -> Self {
        Server {
            listener,
            #[cfg(feature = "http1")]
            http1: http1::Builder::new().into(),
            #[cfg(feature = "http2")]
            http2: http2::Builder::new(TokioExecutor).into(),
        }
    }

    // Use this function to set http protocol.
    // pub fn http1_mut(&mut self) -> &mut http1::Builder {
    //     &mut self.http1
    // }

    // /// Use this function to set http protocol.
    // pub fn http2_mut(&mut self) -> &mut http2::Builder {
    //     &mut self.http2
    // }

    /// Serve a [`Service`]
    #[inline]
    pub async fn serve<S>(self, service: S)
    where
        S: Into<Service>,
    {
        self.try_serve(service).await.unwrap();
    }

    /// Try to serve a [`Service`]
    #[inline]
    pub async fn try_serve<S>(self, service: S) -> IoResult<()>
    where
        S: Into<Service>,
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
    ///     let server = Server::new(TcpListener::bind("127.0.0.1:7878")).serve_with_graceful_shutdown(router, async {
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
    #[inline]
    pub async fn serve_with_graceful_shutdown<S, G>(self, service: S, signal: G, timeout: Option<Duration>)
    where
        S: Into<Service>,
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
        S: Into<Service>,
        G: Future<Output = ()> + Send + 'static,
    {
        let alive_connections = Arc::new(AtomicUsize::new(0));
        let notify = Arc::new(Notify::new());
        let timeout_notify = Arc::new(Notify::new());

        tokio::pin!(signal);

        let mut acceptor = self.listener.into_acceptor().await?;
        for addr in acceptor.local_addrs() {
            tracing::info!( addr = %addr, "listening");
        }

        let service = Arc::new(service.into());
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
                    if let Ok(Accepted {
                        mut conn,
                        local_addr,
                        remote_addr,
                    }) = accepted {
                        let service = service.clone();
                        let alive_connections = alive_connections.clone();
                        let notify = notify.clone();
                        let timeout_notify = timeout_notify.clone();
                        let http1 = self.http1.clone();
                        let http2 = self.http2.clone();
                        tokio::spawn(async move {
                            let version = conn.http_version().await;
                            if let Some(version) = version {
                                match version {
                                    #[cfg(feature = "http1")]
                                    Version::HTTP_10 | Version::HTTP_11 => {
                                        alive_connections.fetch_add(1, Ordering::SeqCst);
                                        let conn = http1.serve_connection(conn, service.hyper_handler(local_addr, remote_addr)).with_upgrades();
                                        if timeout.is_some() {
                                            tokio::select! {
                                                result = conn => {
                                                    if let Err(e) = result {
                                                        tracing::error!(error = ?e, "http1 serve connection failed");
                                                    }
                                                },
                                                _ = timeout_notify.notified() => {}
                                            }
                                        } else if let Err(e) = conn.await {
                                            tracing::error!(error = ?e, "http1 serve connection failed");
                                        }

                                        if alive_connections.fetch_sub(1, Ordering::SeqCst) == 1 {
                                            notify.notify_one();
                                        }
                                    }
                                    #[cfg(feature = "http2")]
                                    Version::HTTP_2 => {
                                        alive_connections.fetch_add(1, Ordering::SeqCst);
                                        let conn = http2.serve_connection(conn, service.hyper_handler(local_addr, remote_addr));
                                        if timeout.is_some() {
                                            tokio::select! {
                                                result = conn => {
                                                    if let Err(e) = result {
                                                        tracing::error!(error = ?e, "http2 serve connection failed");
                                                    }
                                                },
                                                _ = timeout_notify.notified() => {}
                                            }
                                        } else if let Err(e) = conn.await {
                                            tracing::error!(error = ?e, "http2 serve connection failed");
                                        }

                                        if alive_connections.fetch_sub(1, Ordering::SeqCst) == 1 {
                                            notify.notify_one();
                                        }
                                    }
                                    #[cfg(feature = "http3")]
                                    Version::HTTP_3 => {
                                        alive_connections.fetch_add(1, Ordering::SeqCst);
                                        let conn = serve_h3_connection(conn, service.hyper_handler(local_addr, remote_addr));
                                        if timeout.is_some() {
                                            tokio::select! {
                                                result = conn => {
                                                    if let Err(e) = result {
                                                        tracing::error!(error = ?e, "serve connection failed");
                                                    }
                                                },
                                                _ = timeout_notify.notified() => {}
                                            }
                                        } else if let Err(e) = conn.await {
                                            tracing::error!(error = ?e, "serve connection failed");
                                        }

                                        if alive_connections.fetch_sub(1, Ordering::SeqCst) == 1 {
                                            notify.notify_one();
                                        }
                                    }
                                    _ => {
                                        tracing::warn!("unsupported http version");
                                    }
                                }
                            }
                        });
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

#[cfg(feature = "http3")]
async fn serve_h3_connection(
    conn: crate::conn::quic::H3Connection,
    hyper_handler: crate::service::HyperHandler,
) -> Result<(), std::io::Error> {
    loop {
        match conn.accept().await {
            Ok(Some((request, stream))) => {
                tracing::debug!("new request: {:#?}", request);
                tokio::spawn(async {
                    let response = hyper::service::Service::call(&mut hyper_handler, request).await;
                    match response {
                        Ok(response) => match stream.send_response(response).await {
                            Ok(_) => {
                                tracing::debug!("response to connection successful");
                            }
                            Err(e) => {
                                tracing::error!("unable to send response to connection peer: {:?}", e);
                            }
                        },
                        Err(e) => {
                            tracing::error!(error = ?e, "response failed");
                        }
                    }
                    Ok(stream.finish().await?)
                });
            }
            Ok(None) => {
                break;
            }
            Err(err) => {
                tracing::warn!("error on accept {}", err);
                match err.get_error_level() {
                    ErrorLevel::ConnectionError => break,
                    ErrorLevel::StreamError => continue,
                }
            }
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::Other, "connection closed"))
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use crate::prelude::*;

    #[tokio::test]
    async fn test_server() {
        #[handler(internal)]
        async fn hello() -> Result<&'static str, ()> {
            Ok("Hello World")
        }
        #[handler(internal)]
        async fn json(res: &mut Response) {
            #[derive(Serialize, Debug)]
            struct User {
                name: String,
            }
            res.render(Json(User { name: "jobs".into() }));
        }
        let router = Router::new().get(hello).push(Router::with_path("json").get(json));
        let listener = TcpListener::bind("127.0.0.1:0");
        let addr = listener.local_addr();
        let server = tokio::task::spawn(async {
            Server::new(listener).serve(router).await;
        });

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        let base_url = format!("http://{}", addr);
        let client = reqwest::Client::new();
        let result = client.get(&base_url).send().await.unwrap().text().await.unwrap();
        assert_eq!(result, "Hello World");

        let client = reqwest::Client::new();
        let result = client
            .get(format!("{}/json", base_url))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(result, r#"{"name":"jobs"}"#);

        let result = client
            .get(format!("{}/not_exist", base_url))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(result.contains("Not Found"));
        let result = client
            .get(format!("{}/not_exist", base_url))
            .header("accept", "application/json")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(result.contains(r#""code":404"#));
        let result = client
            .get(format!("{}/not_exist", base_url))
            .header("accept", "text/plain")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(result.contains("code:404"));
        let result = client
            .get(format!("{}/not_exist", base_url))
            .header("accept", "application/xml")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(result.contains("<code>404</code>"));
        server.abort();
    }
}
