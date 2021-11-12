//! Server module
use std::io;
use std::net::SocketAddr as StdSocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use hyper::server::accept::Accept;
use hyper::server::conn::AddrIncoming;
use hyper::server::conn::AddrStream;
pub use hyper::Server;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::addr::SocketAddr;
use crate::transport::Transport;

#[cfg(feature = "rustls")]
pub mod rustls;
#[cfg(unix)]
pub mod unix;

#[cfg(feature = "rustls")]
pub use rustls::TlsListener;
#[cfg(unix)]
pub use unix::UnixListener;

/// Listener trait
pub trait Listener: Accept {
    /// Join current Listener with the other.
    fn join<T>(self, other: T) -> JoinedListener<Self, T>
    where
        Self: Sized,
    {
        JoinedListener::new(self, other)
    }
}

/// A IO stream for JoinedListener.
pub enum JoinedStream<A, B> {
    #[allow(missing_docs)]
    A(A),
    #[allow(missing_docs)]
    B(B),
}

impl<A, B> AsyncRead for JoinedStream<A, B>
where
    A: AsyncRead + Send + Unpin + 'static,
    B: AsyncRead + Send + Unpin + 'static,
{
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut() {
            JoinedStream::A(a) => Pin::new(a).poll_read(cx, buf),
            JoinedStream::B(b) => Pin::new(b).poll_read(cx, buf),
        }
    }
}

impl<A, B> AsyncWrite for JoinedStream<A, B>
where
    A: AsyncWrite + Send + Unpin + 'static,
    B: AsyncWrite + Send + Unpin + 'static,
{
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match &mut self.get_mut() {
            JoinedStream::A(a) => Pin::new(a).poll_write(cx, buf),
            JoinedStream::B(b) => Pin::new(b).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut() {
            JoinedStream::A(a) => Pin::new(a).poll_flush(cx),
            JoinedStream::B(b) => Pin::new(b).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut() {
            JoinedStream::A(a) => Pin::new(a).poll_shutdown(cx),
            JoinedStream::B(b) => Pin::new(b).poll_shutdown(cx),
        }
    }
}
impl<A, B> Transport for JoinedStream<A, B>
where
    A: Transport + Send + Unpin + 'static,
    B: Transport + Send + Unpin + 'static,
{
    fn remote_addr(&self) -> Option<SocketAddr> {
        match self {
            JoinedStream::A(stream) => stream.remote_addr(),
            JoinedStream::B(stream) => stream.remote_addr(),
        }
    }
}

/// JoinedListener
pub struct JoinedListener<A, B> {
    a: A,
    b: B,
}

impl<A, B> JoinedListener<A, B> {
    pub(crate) fn new(a: A, b: B) -> Self {
        JoinedListener { a, b }
    }
}
impl<A, B> Accept for JoinedListener<A, B>
where
    A: Transport + Accept + Send + Unpin + 'static,
    B: Transport + Accept + Send + Unpin + 'static,
    A::Conn: AsyncRead + AsyncWrite,
    B::Conn: AsyncRead + AsyncWrite,
{
    type Conn = JoinedStream<A::Conn, B::Conn>;
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let pin = self.get_mut();
        if fastrand::bool() {
            match Pin::new(&mut pin.a).poll_accept(cx) {
                Poll::Ready(Some(result)) => Poll::Ready(Some(
                    result
                        .map(JoinedStream::A)
                        .map_err(|_| io::Error::from(io::ErrorKind::Other)),
                )),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => match Pin::new(&mut pin.b).poll_accept(cx) {
                    Poll::Ready(Some(result)) => Poll::Ready(Some(
                        result
                            .map(JoinedStream::B)
                            .map_err(|_| io::Error::from(io::ErrorKind::Other)),
                    )),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                },
            }
        } else {
            match Pin::new(&mut pin.b).poll_accept(cx) {
                Poll::Ready(Some(result)) => Poll::Ready(Some(
                    result
                        .map(JoinedStream::B)
                        .map_err(|_| io::Error::from(io::ErrorKind::Other)),
                )),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => match Pin::new(&mut pin.a).poll_accept(cx) {
                    Poll::Ready(Some(result)) => Poll::Ready(Some(
                        result
                            .map(JoinedStream::A)
                            .map_err(|_| io::Error::from(io::ErrorKind::Other)),
                    )),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                },
            }
        }
    }
}

/// TcpListener
pub struct TcpListener {
    incoming: AddrIncoming,
}
impl TcpListener {
    /// Create `TcpListener`.
    pub fn new(incoming: AddrIncoming) -> Self {
        Self { incoming }
    }
    /// Bind to socket address.
    pub fn bind(addr: impl Into<StdSocketAddr>) -> Result<Self, hyper::Error> {
        let mut incoming = AddrIncoming::bind(&addr.into())?;
        incoming.set_nodelay(true);

        Ok(TcpListener { incoming })
    }
}
impl Listener for TcpListener {}
impl Accept for TcpListener {
    type Conn = AddrStream;
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        Pin::new(&mut self.get_mut().incoming).poll_accept(cx)
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
            Server::builder(TcpListener::bind(([0, 0, 0, 0], 7979)).unwrap())
                .serve(Service::new(router))
                .await
                .unwrap();
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
