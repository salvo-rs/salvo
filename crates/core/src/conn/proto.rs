use std::cmp;
use std::error::Error as StdError;
use std::future::Future;
use std::io::IoSlice;
use std::io::Result as IoResult;
use std::marker::Unpin;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{self, Context, Poll};
use std::time::Duration;

use bytes::{Buf, Bytes};

use http::{Request, Response};
use hyper::service::Service;
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};
use tokio::sync::{oneshot, Notify};
use tokio_util::either::Either;
use tokio_util::sync::CancellationToken;

use crate::http::body::{Body, HyperBody};
use crate::rt::TokioIo;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(feature = "http2")]
use crate::rt::TokioExecutor;
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
        mut socket: I,
        service: S,
        server_shutdown_token: CancellationToken,
        idle_connection_timeout: Option<Duration>,
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
        #[derive(Debug)]
        enum Protocol {
            H1,
            H2,
        }

        let conn_shutdown_token = CancellationToken::new();
        let mut buf = [0; 24];
        let protocol = if socket.read_exact(&mut buf).await.is_ok() {
            if buf == H2_PREFACE {
                Protocol::H2
            } else {
                Protocol::H1
            }
        } else {
            Protocol::H1
        };
        let socket = Rewind::new_buffered(Bytes::from(buf.to_vec()), socket);

        let socket = match idle_connection_timeout {
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

        match protocol {
            Protocol::H1 => {
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
                        _ = server_shutdown_token.cancelled() => {}
                    }

                    // Init graceful shutdown for connection (`GOAWAY` for `HTTP/2` or disabling `keep-alive` for `HTTP/1`)
                    Pin::new(&mut conn).graceful_shutdown();
                    conn.await.ok();
                }
            }
            Protocol::H2 => {
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
                        _ = server_shutdown_token.cancelled() => {}
                    }

                    // Init graceful shutdown for connection (`GOAWAY` for `HTTP/2` or disabling `keep-alive` for `HTTP/1`)
                    Pin::new(&mut conn).graceful_shutdown();
                    conn.await.ok();
                }
            }
        }

        Ok(())
    }
}

// from https://github.com/hyperium/hyper-util/pull/11/files#diff-1bd3ef8e9a23396b76bdb4ec6ab5aba4c48dd0511d287e485148a90170c6b4fd
/// Combine a buffer with an IO, rewinding reads to use the buffer.
#[derive(Debug)]
struct Rewind<T> {
    pre: Option<Bytes>,
    inner: T,
}

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

#[pin_project]
struct ClosingInactiveConnection<T> {
    #[pin]
    inner: T,
    #[pin]
    alive: Arc<Notify>,
    timeout: Duration,
    stop_tx: oneshot::Sender<()>,
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
        let (stop_tx, stop_rx) = oneshot::channel();
        tokio::spawn({
            let alive = alive.clone();

            async move {
                let check_timeout = async {
                    loop {
                        match tokio::time::timeout(timeout, alive.notified()).await {
                            Ok(()) => {}
                            Err(_) => {
                                f().await;
                            }
                        }
                    }
                };
                tokio::select! {
                    _ = stop_rx => {},
                    _ = check_timeout => {}
                }
            }
        });
        Self {
            inner,
            alive,
            timeout,
            stop_tx,
        }
    }
}
