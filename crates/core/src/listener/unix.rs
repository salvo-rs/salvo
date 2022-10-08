//! UnixListener module
use std::io::{Error as IoError, Result as IoResult};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use hyper::server::accept::Accept;
pub use hyper::Server;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::Listener;
use crate::addr::SocketAddr;
use crate::transport::Transport;

/// Unix domain socket listener.
#[cfg(unix)]
pub struct UnixListener {
    incoming: tokio::net::UnixListener,
}
#[cfg(unix)]
impl UnixListener {
    /// Creates a new `UnixListener` bind to the specified path.
    #[inline]
    pub fn bind(path: impl AsRef<Path>) -> UnixListener {
        Self::try_bind(path).unwrap()
    }
    /// Creates a new `UnixListener` bind to the specified path.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime.
    #[inline]
    pub fn try_bind(path: impl AsRef<Path>) -> IoResult<UnixListener> {
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
    type Error = IoError;

    #[inline]
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
    #[inline]
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr.clone())
    }
}

impl UnixStream {
    #[inline]
    fn new(inner_stream: tokio::net::UnixStream, remote_addr: SocketAddr) -> Self {
        UnixStream {
            inner_stream,
            remote_addr,
        }
    }
}

impl AsyncRead for UnixStream {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<IoResult<()>> {
        Pin::new(&mut self.get_mut().inner_stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for UnixStream {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        Pin::new(&mut self.get_mut().inner_stream).poll_write(cx, buf)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        Pin::new(&mut self.get_mut().inner_stream).poll_flush(cx)
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        Pin::new(&mut self.get_mut().inner_stream).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    impl Stream for UnixListener {
        type Item = Result<UnixStream, IoError>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.poll_accept(cx)
        }
    }
    #[tokio::test]
    async fn test_unix_listener() {
        let sock_file = "/tmp/test-salvo.sock";
        let mut listener = UnixListener::bind(sock_file);

        tokio::spawn(async move {
            let mut stream = tokio::net::UnixStream::connect(sock_file).await.unwrap();
            stream.write_i32(518).await.unwrap();
        });

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 518);
        std::fs::remove_file(sock_file).unwrap();
    }
}
