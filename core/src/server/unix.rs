//! UnixListener module
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use hyper::server::accept::Accept;
pub use hyper::Server;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::Listener;
use crate::addr::SocketAddr;
use crate::transport::Transport;

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
            Poll::Ready(Ok((stream, remote_addr))) => {
                Poll::Ready(Some(Ok(UnixStream::new(stream, remote_addr.into()))))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Some(Err(err))),
            Poll::Pending => Poll::Pending,
        }
    }
}
/// UnixStream
pub struct UnixStream {
    inner_stream: tokio::net::UnixStream,
    remote_addr: SocketAddr,
}
impl Transport for UnixStream {
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr.clone())
    }
}

impl UnixStream {
    fn new(inner_stream: tokio::net::UnixStream, remote_addr: SocketAddr) -> Self {
        UnixStream {
            inner_stream,
            remote_addr,
        }
    }
}

impl AsyncRead for UnixStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner_stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for UnixStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner_stream).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner_stream).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner_stream).poll_shutdown(cx)
    }
}
