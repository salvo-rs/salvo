//! Listener trait and it's implements.
use std::io::{self, Error as IoError, ErrorKind};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::async_trait;
use crate::conn::SocketAddr;

use super::{Accepted, Acceptor, Listener};

/// A I/O stream for JoinedListener.
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
    #[inline]
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
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match &mut self.get_mut() {
            JoinedStream::A(a) => Pin::new(a).poll_write(cx, buf),
            JoinedStream::B(b) => Pin::new(b).poll_write(cx, buf),
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut() {
            JoinedStream::A(a) => Pin::new(a).poll_flush(cx),
            JoinedStream::B(b) => Pin::new(b).poll_flush(cx),
        }
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut() {
            JoinedStream::A(a) => Pin::new(a).poll_shutdown(cx),
            JoinedStream::B(b) => Pin::new(b).poll_shutdown(cx),
        }
    }
}

/// JoinedListener
pub struct JoinedListener<A, B> {
    a: A,
    b: B,
}

impl<A, B> JoinedListener<A, B> {
    #[inline]
    pub(crate) fn new(a: A, b: B) -> Self {
        JoinedListener { a, b }
    }
}
impl<A, B> Listener for JoinedListener<A, B>
where
    A: Listener + Send + Unpin + 'static,
    B: Listener + Send + Unpin + 'static,
    A::Conn: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    B::Conn: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
}

#[async_trait]
impl<A, B> Acceptor for JoinedListener<A, B>
where
    A: Listener + Send + Unpin + 'static,
    B: Listener + Send + Unpin + 'static,
    A::Conn: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    B::Conn: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Conn = JoinedStream<A::Conn, B::Conn>;
    type Error = IoError;

    fn local_addrs(&self) -> Vec<&SocketAddr> {
        self.a
            .local_addrs()
            .into_iter()
            .chain(self.b.local_addrs().into_iter())
            .collect()
    }

    #[inline]
    async fn accept(&self) -> Result<Accepted<Self::Conn>, Self::Error> {
        tokio::select! {
            trans = self.a.accept() => {
                let Accepted {
                    stream, local_addr, remote_addr
                 } = trans.map_err(|_|IoError::new(ErrorKind::Other, "a accept error"))?;
                Ok(Accepted {
                    stream: JoinedStream::A(stream),
                    local_addr,
                    remote_addr
                })
            }
            trans = self.b.accept() => {
                let Accepted {
                    stream, local_addr, remote_addr
                 } = trans.map_err(|_|IoError::new(ErrorKind::Other, "b accept error"))?;
                Ok(Accepted {
                    stream: JoinedStream::B(stream),
                    local_addr,
                    remote_addr
                })
            }
        }
    }
}
