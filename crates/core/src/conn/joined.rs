//! JoinListener and its implementations.
use std::fmt::{self, Debug, Formatter};
use std::io::Result as IoResult;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::{BoxFuture, FutureExt};
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

use crate::conn::{Coupler, Holding, HttpBuilder};
use crate::fuse::ArcFuseFactory;
use crate::service::HyperHandler;

use super::{Accepted, Acceptor, Listener};

/// An Coupler for JoinedListener.
pub enum JoinedCoupler<A, B> {
    #[allow(missing_docs)]
    A(A),
    #[allow(missing_docs)]
    B(B),
}

impl<A, B> Coupler for JoinedCoupler<A, B>
where
    A: Coupler + Unpin + 'static,
    B: Coupler + Unpin + 'static,
{
    type Stream = JoinedStream<A::Stream, B::Stream>;

    fn couple(
        &self,
        stream: Self::Stream,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        graceful_stop_token: Option<CancellationToken>,
    ) -> BoxFuture<'static, IoResult<()>> {
        match (self, stream) {
            (Self::A(a), JoinedStream::A(stream)) => a
                .couple(stream, handler, builder, graceful_stop_token)
                .boxed(),
            (Self::B(b), JoinedStream::B(stream)) => b
                .couple(stream, handler, builder, graceful_stop_token)
                .boxed(),
            _ => unreachable!(),
        }
    }
}

impl<A, B> Debug for JoinedCoupler<A, B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinedCoupler").finish()
    }
}

/// An I/O stream for JoinedListener.
pub enum JoinedStream<A, B> {
    #[allow(missing_docs)]
    A(A),
    #[allow(missing_docs)]
    B(B),
}

impl<A, B> Debug for JoinedStream<A, B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinedStream").finish()
    }
}

impl<A, B> AsyncRead for JoinedStream<A, B>
where
    A: AsyncRead + Send + Unpin + 'static,
    B: AsyncRead + Send + Unpin + 'static,
{
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<IoResult<()>> {
        match &mut self.get_mut() {
            Self::A(a) => Pin::new(a).poll_read(cx, buf),
            Self::B(b) => Pin::new(b).poll_read(cx, buf),
        }
    }
}

impl<A, B> AsyncWrite for JoinedStream<A, B>
where
    A: AsyncWrite + Send + Unpin + 'static,
    B: AsyncWrite + Send + Unpin + 'static,
{
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        match &mut self.get_mut() {
            Self::A(a) => Pin::new(a).poll_write(cx, buf),
            Self::B(b) => Pin::new(b).poll_write(cx, buf),
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        match &mut self.get_mut() {
            Self::A(a) => Pin::new(a).poll_flush(cx),
            Self::B(b) => Pin::new(b).poll_flush(cx),
        }
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        match &mut self.get_mut() {
            Self::A(a) => Pin::new(a).poll_shutdown(cx),
            Self::B(b) => Pin::new(b).poll_shutdown(cx),
        }
    }
}

/// `JoinedListener` is a listener that can join two listeners.
#[pin_project]
pub struct JoinedListener<A, B> {
    #[pin]
    a: A,
    #[pin]
    b: B,
}

impl<A: Debug, B: Debug> Debug for JoinedListener<A, B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinedListener")
            .field("a", &self.a)
            .field("b", &self.b)
            .finish()
    }
}

impl<A, B> JoinedListener<A, B> {
    /// Create a new `JoinedListener`.
    #[inline]
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}
impl<A, B> Listener for JoinedListener<A, B>
where
    A: Listener + Send + Unpin + 'static,
    B: Listener + Send + Unpin + 'static,
    A::Acceptor: Acceptor + Send + Unpin + 'static,
    B::Acceptor: Acceptor + Send + Unpin + 'static,
{
    type Acceptor = JoinedAcceptor<A::Acceptor, B::Acceptor>;

    async fn try_bind(self) -> crate::Result<Self::Acceptor> {
        let a = self.a.try_bind().await?;
        let b = self.b.try_bind().await?;
        let holdings = a
            .holdings()
            .iter()
            .chain(b.holdings().iter())
            .cloned()
            .collect();
        Ok(JoinedAcceptor { a, b, holdings })
    }
}

/// `JoinedAcceptor` is an acceptor that can accept connections from two different acceptors.
pub struct JoinedAcceptor<A, B> {
    a: A,
    b: B,
    holdings: Vec<Holding>,
}

impl<A: Debug, B: Debug> Debug for JoinedAcceptor<A, B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinedAcceptor")
            .field("a", &self.a)
            .field("b", &self.b)
            .field("holdings", &self.holdings)
            .finish()
    }
}

impl<A, B> JoinedAcceptor<A, B>
where
    A: Acceptor,
    B: Acceptor,
{
    /// Create a new `JoinedAcceptor`.
    pub fn new(a: A, b: B) -> Self {
        let holdings = a
            .holdings()
            .iter()
            .chain(b.holdings().iter())
            .cloned()
            .collect();
        Self { a, b, holdings }
    }
}

impl<A, B> Acceptor for JoinedAcceptor<A, B>
where
    A: Acceptor + Send + Unpin + 'static,
    B: Acceptor + Send + Unpin + 'static,
    A::Coupler: Coupler<Stream = A::Stream> + Unpin + 'static,
    B::Coupler: Coupler<Stream = B::Stream> + Unpin + 'static,
    A::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    B::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Coupler = JoinedCoupler<A::Coupler, B::Coupler>;
    type Stream = JoinedStream<A::Stream, B::Stream>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        tokio::select! {
            accepted = self.a.accept(fuse_factory.clone()) => {
                Ok(accepted?.map_into(JoinedCoupler::A, JoinedStream::A))
            }
            accepted = self.b.accept(fuse_factory) => {
                Ok(accepted?.map_into(JoinedCoupler::B, JoinedStream::B))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;
    use crate::conn::TcpListener;

    #[tokio::test]
    async fn test_joined_listener() {
        let addr1 = std::net::SocketAddr::from(([127, 0, 0, 1], 6978));
        let addr2 = std::net::SocketAddr::from(([127, 0, 0, 1], 6979));

        let mut acceptor = TcpListener::new(addr1)
            .join(TcpListener::new(addr2))
            .bind()
            .await;
        tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr1).await.unwrap();
            stream.write_i32(50).await.unwrap();

            let mut stream = TcpStream::connect(addr2).await.unwrap();
            stream.write_i32(100).await.unwrap();
        });
        let Accepted { mut stream, .. } = acceptor.accept(None).await.unwrap();
        let first = stream.read_i32().await.unwrap();
        let Accepted { mut stream, .. } = acceptor.accept(None).await.unwrap();
        let second = stream.read_i32().await.unwrap();
        assert_eq!(first + second, 150);
    }
}
