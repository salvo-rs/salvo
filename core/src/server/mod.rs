//! Server module
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(target_os = "linux")]
use hyper::server::accept;
use hyper::server::accept::Accept;
use hyper::server::conn::AddrIncoming;
use hyper::server::conn::AddrStream;
pub use hyper::Server;

#[cfg(feature = "tls")]
pub mod tls;

#[cfg(feature = "tls")]
pub use tls::TlsListener;
#[cfg(target_os = "linux")]
use crate::transport::LiftIo;

/// TcpListener
pub struct TcpListener {
    incoming: AddrIncoming,
}
impl TcpListener {
    /// Bind to socket address.
    pub fn bind(addr: impl Into<SocketAddr>) -> Result<Self, hyper::Error> {
        let addr = addr.into();
        let mut incoming = AddrIncoming::bind(&addr)?;
        incoming.set_nodelay(true);

        Ok(TcpListener { incoming })
    }
}

impl Accept for TcpListener {
    type Conn = AddrStream;
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        Pin::new(&mut self.get_mut().incoming).poll_accept(cx)
    }
}

#[cfg(target_os = "linux")]
pub struct UnixListener {
    incoming: LiftIo<AddrIncoming>,
}
#[cfg(target_os = "linux")]
impl UnixListener {
    /// Creates a new `UnixListener` bound to the specified path.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn bind<P>(path: P) -> io::Result<UnixListener>
    where
        P: AsRef<Path>,
    {
        let mut incoming = tokio::net::UnixListener::bind(path)?;
        incoming.set_nodelay(true);
        let incoming = accept::from_stream(incoming.map_ok(LiftIo).into_stream());
        Ok(UnixListener { incoming })
    }
}

#[cfg(target_os = "linux")]
impl Accept for UnixListener {
    type Conn = LiftIo<AddrIncoming>;
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        Pin::new(&mut self.get_mut()).poll_accept(cx)
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use crate::prelude::*;

    #[tokio::test]
    async fn test_server() {
        #[fn_handler]
        async fn hello_world() -> Result<&'static str, ()> {
            Ok("Hello World")
        }
        #[fn_handler]
        async fn json(res: &mut Response) {
            #[derive(Serialize, Debug)]
            struct User {
                name: String,
            }
            res.render_json(&User { name: "jobs".into() });
        }
        let router = Router::new().get(hello_world).push(Router::with_path("json").get(json));

        tokio::task::spawn(async {
            Server::new(router).bind(([0, 0, 0, 0], 7979)).await;
        });

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
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

        let client = reqwest::Client::new();
        let result = client
            .get("http://127.0.0.1:7979/json")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(result, r#"{"name":"jobs"}"#);

        let result = client
            .get("http://127.0.0.1:7979/not_exist")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(result.contains("Not Found"));
        let result = client
            .get("http://127.0.0.1:7979/not_exist")
            .header("accept", "application/json")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(result.contains(r#""code":404"#));
        let result = client
            .get("http://127.0.0.1:7979/not_exist")
            .header("accept", "text/plain")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(result.contains("code:404"));
        let result = client
            .get("http://127.0.0.1:7979/not_exist")
            .header("accept", "application/xml")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(result.contains("<code>404</code>"));
    }
}
