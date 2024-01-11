use std::cmp;
use std::error::Error as StdError;
use std::future::Future;
use std::io::{Error as IoError, ErrorKind, IoSlice, Result as IoResult};
use std::marker::{PhantomPinned, Unpin};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{self, ready, Context, Poll};
use std::time::Duration;

use bytes::{Buf, Bytes};

use http::{Request, Response, Version};
use hyper::service::Service;
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::Notify;
use tokio_util::either::Either;
use tokio_util::sync::CancellationToken;

use crate::http::body::{Body, HyperBody};
#[cfg(any(feature = "http1", feature = "http2"))]
use crate::rt::tokio::TokioIo;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(feature = "http2")]
use crate::rt::tokio::TokioExecutor;
#[cfg(feature = "http1")]
use hyper::server::conn::http1;
#[cfg(feature = "http2")]
use hyper::server::conn::http2;

#[cfg(feature = "quinn")]
use crate::conn::quinn;

const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

#[doc(hidden)]
pub struct HttpBuilder {
    #[cfg(feature = "http1")]
    pub(crate) http1: http1::Builder,
    #[cfg(feature = "http2")]
    pub(crate) http2: http2::Builder<TokioExecutor>,
    #[cfg(feature = "quinn")]
    pub(crate) quinn: quinn::Builder,
}
impl HttpBuilder {
    /// Bind a connection together with a [`Service`].
    pub async fn serve_connection<I, S, B>(
        &self,
        socket: I,
        #[allow(unused_variables)] service: S,
        idle_timeout: Option<Duration>,
    ) -> Result<()>
    where
        S: Service<Request<HyperBody>, Response = Response<B>> + Send,
        S::Future: Send + 'static,
        S::Error: Into<Box<dyn StdError + Send + Sync>>,
        B: Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<Box<dyn StdError + Send + Sync>>,
        I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let conn_shutdown_token = CancellationToken::new();
        #[cfg(all(feature = "http1", feature = "http2"))]
        let (version, socket) = read_version(socket).await?;
        #[cfg(all(not(feature = "http1"), not(feature = "http2")))]
        let version = Version::HTTP_11; // Just make the compiler happy.
        #[cfg(all(feature = "http1", not(feature = "http2")))]
        let version = Version::HTTP_11;
        #[cfg(all(not(feature = "http1"), feature = "http2"))]
        let version = Version::HTTP_2;
        #[allow(unused_variables)]
        let socket = match idle_timeout {
            Some(timeout) => Either::Left(ClosingInactiveConnection::new(socket, timeout, {
                let conn_shutdown_token = conn_shutdown_token.clone();

                move || {
                    let conn_shutdown_token = conn_shutdown_token.clone();
                    async move {
                        conn_shutdown_token.cancel();
                    }
                }
            })),
            None => Either::Right(socket),
        };

        match version {
            Version::HTTP_10 | Version::HTTP_11 => {
                #[cfg(not(feature = "http1"))]
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "http1 feature not enabled").into());
                #[cfg(feature = "http1")]
                {
                    let mut conn = self
                        .http1
                        .serve_connection(TokioIo::new(socket), service)
                        .with_upgrades();

                    tokio::select! {
                        _ = &mut conn => {
                            // Connection completed successfully.
                            return Ok(());
                        },
                        _ = conn_shutdown_token.cancelled() => {
                            tracing::info!("closing connection due to inactivity");
                        }
                    }

                    // Init graceful shutdown for connection (`GOAWAY` for `HTTP/2` or disabling `keep-alive` for `HTTP/1`)
                    Pin::new(&mut conn).graceful_shutdown();
                    conn.await.ok();
                }
            }
            Version::HTTP_2 => {
                #[cfg(not(feature = "http2"))]
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "http2 feature not enabled").into());
                #[cfg(feature = "http2")]
                {
                    let mut conn = self.http2.serve_connection(TokioIo::new(socket), service);
                    tokio::select! {
                        _ = &mut conn => {
                            // Connection completed successfully.
                            return Ok(());
                        },
                        _ = conn_shutdown_token.cancelled() => {
                            tracing::info!("closing connection due to inactivity");
                        }
                    }

                    // Init graceful shutdown for connection (`GOAWAY` for `HTTP/2` or disabling `keep-alive` for `HTTP/1`)
                    Pin::new(&mut conn).graceful_shutdown();
                    conn.await.ok();
                }
            }
            _ => {
                tracing::info!("unsupported protocol version: {:?}", version);
            }
        }

        Ok(())
    }
}

#[pin_project]
struct ClosingInactiveConnection<T> {
    #[pin]
    inner: T,
    #[pin]
    alive: Arc<Notify>,
    timeout: Duration,
}

impl<T> AsyncRead for ClosingInactiveConnection<T>
where
    T: AsyncRead,
{
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<IoResult<()>> {
        let this = self.project();

        match this.inner.poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                this.alive.notify_waiters();
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> AsyncWrite for ClosingInactiveConnection<T>
where
    T: AsyncWrite,
{
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        let this = self.project();
        this.alive.notify_waiters();
        this.inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = self.project();
        this.alive.notify_waiters();
        this.inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = self.project();
        this.alive.notify_waiters();
        this.inner.poll_shutdown(cx)
    }

    fn poll_write_vectored(self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[IoSlice<'_>]) -> Poll<IoResult<usize>> {
        let this = self.project();
        this.alive.notify_waiters();
        this.inner.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

impl<T> ClosingInactiveConnection<T> {
    fn new<F, Fut>(inner: T, timeout: Duration, mut f: F) -> ClosingInactiveConnection<T>
    where
        F: Send + FnMut() -> Fut + 'static,
        Fut: Future + Send + 'static,
    {
        let alive = Arc::new(Notify::new());
        tokio::spawn({
            let alive = alive.clone();
            async move {
                loop {
                    if tokio::time::timeout(timeout, alive.notified()).await.is_err() {
                        f().await;
                        break;
                    }
                }
            }
        });
        Self { inner, alive, timeout }
    }
}

#[allow(dead_code)]
#[allow(clippy::future_not_send)]
pub(crate) async fn read_version<'a, A>(mut reader: A) -> IoResult<(Version, Rewind<A>)>
where
    A: AsyncRead + Unpin,
{
    let mut buf = [0; 24];
    let (version, buf) = ReadVersion {
        reader: &mut reader,
        buf: ReadBuf::new(&mut buf),
        version: Version::HTTP_11,
        _pin: PhantomPinned,
    }
    .await?;
    Ok((version, Rewind::new_buffered(Bytes::from(buf), reader)))
}

#[derive(Debug)]
#[pin_project]
#[must_use = "futures do nothing unless you `.await` or poll them"]
struct ReadVersion<'a, A: ?Sized> {
    reader: &'a mut A,
    buf: ReadBuf<'a>,
    version: Version,
    // Make this future `!Unpin` for compatibility with async trait methods.
    #[pin]
    _pin: PhantomPinned,
}

impl<A> Future for ReadVersion<'_, A>
where
    A: AsyncRead + Unpin + ?Sized,
{
    type Output = IoResult<(Version, Vec<u8>)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<(Version, Vec<u8>)>> {
        let this = self.project();

        while this.buf.remaining() != 0 {
            if this.buf.filled() != &H2_PREFACE[0..this.buf.filled().len()] {
                return Poll::Ready(Ok((*this.version, this.buf.filled().to_vec())));
            }
            // if our buffer is empty, then we need to read some data to continue.
            let rem = this.buf.remaining();
            ready!(Pin::new(&mut *this.reader).poll_read(cx, this.buf))?;
            if this.buf.remaining() == rem {
                return Err(IoError::new(ErrorKind::UnexpectedEof, "early eof")).into();
            }
        }
        if this.buf.filled() == H2_PREFACE {
            *this.version = Version::HTTP_2;
        }
        return Poll::Ready(Ok((*this.version, this.buf.filled().to_vec())));
    }
}

// from https://github.com/hyperium/hyper-util/pull/11/files#diff-1bd3ef8e9a23396b76bdb4ec6ab5aba4c48dd0511d287e485148a90170c6b4fd
/// Combine a buffer with an IO, rewinding reads to use the buffer.
#[derive(Debug)]
pub(crate) struct Rewind<T> {
    pre: Option<Bytes>,
    inner: T,
}
#[allow(dead_code)]
impl<T> Rewind<T> {
    fn new_buffered(buf: Bytes, io: T) -> Self {
        Rewind {
            pre: Some(buf),
            inner: io,
        }
    }
}

impl<T> AsyncRead for Rewind<T>
where
    T: AsyncRead + Unpin,
{
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<IoResult<()>> {
        if let Some(mut prefix) = self.pre.take() {
            // If there are no remaining bytes, let the bytes get dropped.
            if !prefix.is_empty() {
                let copy_len = cmp::min(prefix.len(), buf.remaining());
                // TODO: There should be a way to do following two lines cleaner...
                buf.put_slice(&prefix[..copy_len]);
                prefix.advance(copy_len);
                // Put back what's left
                if !prefix.is_empty() {
                    self.pre = Some(prefix);
                }
                return Poll::Ready(Ok(()));
            }
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<T> AsyncWrite for Rewind<T>
where
    T: AsyncWrite + Unpin,
{
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<IoResult<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<IoResult<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<IoResult<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}
