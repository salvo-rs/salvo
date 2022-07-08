//! Server module
use std::error::Error as StdError;
use std::future::Future;

use hyper::Server as HyperServer;

use crate::transport::Transport;
use crate::{Listener, Service};

/// HTTP Server
///
/// A `Server` is created to listen on a port, parse HTTP requests, and hand them off to a [`Service`].
pub struct Server<L> {
    listener: L,
}

impl<L> Server<L>
where
    L: Listener,
    L::Conn: Transport + Send + Unpin + 'static,
    L::Error: Into<Box<(dyn StdError + Send + Sync + 'static)>>,
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
        Server { listener }
    }

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
    pub async fn try_serve<S>(self, service: S) -> Result<(), hyper::Error>
    where
        S: Into<Service>,
    {
        HyperServer::builder(self.listener).serve(service.into()).await
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
    /// async fn hello_world(res: &mut Response) {
    ///     res.render("Hello World!");
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, rx) = oneshot::channel();
    ///     let router = Router::new().get(hello_world);
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
    pub async fn serve_with_graceful_shutdown<S, G>(self, addr: S, signal: G)
    where
        S: Into<Service>,
        G: Future<Output = ()> + Send + 'static,
    {
        self.try_serve_with_graceful_shutdown(addr, signal).await.unwrap();
    }

    /// Serve with graceful shutdown signal.
    #[inline]
    pub async fn try_serve_with_graceful_shutdown<S, G>(self, service: S, signal: G) -> Result<(), hyper::Error>
    where
        S: Into<Service>,
        G: Future<Output = ()> + Send + 'static,
    {
        let server = HyperServer::builder(self.listener).serve(service.into());
        if let Err(err) = server.with_graceful_shutdown(signal).await {
            tracing::error!("server error: {}", err);
            Err(err)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use crate::prelude::*;

    #[tokio::test]
    async fn test_server() {
        #[handler(internal)]
        async fn hello_world() -> Result<&'static str, ()> {
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
        let router = Router::new().get(hello_world).push(Router::with_path("json").get(json));
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
