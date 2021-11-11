//! Server module
use std::io;
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use hyper::server::accept::Accept;
use hyper::server::conn::AddrIncoming;
use hyper::server::conn::AddrStream;
pub use hyper::Server;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
#[cfg(unix)]
use tokio::net::UnixStream;

use crate::transport::Transport;

#[cfg(feature = "tls")]
mod tls;

#[cfg(feature = "tls")]
pub use tls::TlsListener;

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
    A: AsyncWrite + AsyncRead + Send + Unpin + 'static,
    B: AsyncWrite + AsyncRead + Send + Unpin + 'static,
{
    fn remote_addr(&self) -> Option<SocketAddr> {
        None
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
    A: Accept + Send + Unpin + 'static,
    B: Accept + Send + Unpin + 'static,
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
    /// Bind to socket address.
    pub fn bind(addr: impl Into<SocketAddr>) -> Result<Self, hyper::Error> {
        let addr = addr.into();
        let mut incoming = AddrIncoming::bind(&addr)?;
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

/// UnixListener
#[cfg(unix)]
pub struct UnixListener {
    incoming: tokio::net::UnixListener,
}
#[cfg(unix)]
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
        Ok(UnixListener {
            incoming: tokio::net::UnixListener::bind(path)?,
        })
    }
}

#[cfg(unix)]
impl Listener for UnixListener {}
#[cfg(unix)]
impl Accept for UnixListener {
    type Conn = UnixStream;
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        match self.incoming.poll_accept(cx) {
            Poll::Ready(Ok((stream, _))) => Poll::Ready(Some(Ok(stream))),
            Poll::Ready(Err(err)) => Poll::Ready(Some(Err(err))),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(unix)]
impl Transport for UnixStream {
    fn remote_addr(&self) -> Option<SocketAddr> {
        None
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
