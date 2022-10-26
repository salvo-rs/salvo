//! JoinListener and it's implements.
use std::io::{self, Result as IoResult};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::async_trait;
use crate::conn::addr::LocalAddr;
use crate::conn::HttpBuilders;
use crate::http::{HttpConnection, Version};
use crate::service::HyperHandler;

use super::{Accepted, Acceptor, IntoAcceptor, Listener};

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
#[pin_project]
pub struct JoinedListener<A, B> {
    #[pin]
    a: A,
    #[pin]
    b: B,
}

impl<A, B> JoinedListener<A, B> {
    /// Create a new `JoinedListener`.
    #[inline]
    pub fn new(a: A, b: B) -> Self {
        JoinedListener { a, b }
    }
}
#[async_trait]
impl<A, B> IntoAcceptor for JoinedListener<A, B>
where
    A: Listener + Send + Unpin + 'static,
    B: Listener + Send + Unpin + 'static,
    A::Acceptor: Acceptor + Send + Unpin + 'static,
    B::Acceptor: Acceptor + Send + Unpin + 'static,
{
    type Acceptor = JoinedAcceptor<A::Acceptor, B::Acceptor>;
    async fn into_acceptor(self) -> IoResult<Self::Acceptor> {
        Ok(JoinedAcceptor {
            a: self.a.into_acceptor().await?,
            b: self.b.into_acceptor().await?,
        })
    }
}

pub struct JoinedAcceptor<A, B> {
    a: A,
    b: B,
}

#[async_trait]
impl<A, B> HttpConnection for JoinedStream<A, B>
where
    A: HttpConnection + Send,
    B: HttpConnection + Send,
{
    async fn version(&mut self) -> Option<Version> {
        match self {
            JoinedStream::A(a) => a.version().await,
            JoinedStream::B(b) => b.version().await,
        }
    }
    async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()> {
        match self {
            JoinedStream::A(a) => a.serve(handler, builders).await,
            JoinedStream::B(b) => b.serve(handler, builders).await,
        }
    }
}

#[async_trait]
impl<A, B> Acceptor for JoinedAcceptor<A, B>
where
    A: Acceptor + Send + Unpin + 'static,
    B: Acceptor + Send + Unpin + 'static,
    A::Conn: HttpConnection + AsyncRead + AsyncWrite + Send + Unpin + 'static,
    B::Conn: HttpConnection + AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Conn = JoinedStream<A::Conn, B::Conn>;

    #[inline]
    fn local_addrs(&self) -> Vec<LocalAddr> {
        self.a
            .local_addrs()
            .into_iter()
            .chain(self.b.local_addrs().into_iter())
            .collect()
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        tokio::select! {
            accepted = self.a.accept() => {
                Ok(accepted?.map_conn(JoinedStream::A))
            }
            accepted = self.b.accept() => {
                Ok(accepted?.map_conn(JoinedStream::B))
            }
        }
    }
}
