use tokio_util::sync::CancellationToken;

use std::io::{Error as IoError, ErrorKind, IoSlice, Result as IoResult};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::conn::HttpBuilder;
use crate::fuse::{ArcFusewire, FuseEvent};
use crate::http::HttpConnection;
use crate::service::HyperHandler;

/// A stream that can be fused.
#[pin_project]
pub struct StraightStream<C> {
    #[pin]
    inner: C,
    fusewire: Option<ArcFusewire>,
}

impl<C> StraightStream<C>
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    /// Create a new `StraightStream`.
    pub fn new(inner: C, fusewire: Option<ArcFusewire>) -> Self {
        Self { inner, fusewire }
    }
}

impl<C> HttpConnection for StraightStream<C>
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    async fn serve(
        self,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        graceful_stop_token: Option<CancellationToken>,
    ) -> std::io::Result<()> {
        let fusewire = self.fusewire.clone();
        if let Some(fusewire) = &fusewire {
            fusewire.event(FuseEvent::Alive);
        }
        builder
            .serve_connection(self, handler, fusewire, graceful_stop_token)
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))
    }
    fn fusewire(&self) -> Option<ArcFusewire> {
        self.fusewire.clone()
    }
}

impl<C> AsyncRead for StraightStream<C>
where
    C: AsyncRead,
{
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<IoResult<()>> {
        let this = self.project();
        let remaining = buf.remaining();
        match this.inner.poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                if let Some(fusewire) = &this.fusewire {
                    fusewire.event(FuseEvent::ReadData(remaining - buf.remaining()));
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => {
                if let Some(fusewire) = &this.fusewire {
                    fusewire.event(FuseEvent::Alive);
                }
                Poll::Pending
            }
        }
    }
}

impl<C> AsyncWrite for StraightStream<C>
where
    C: AsyncWrite,
{
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        let this = self.project();
        match this.inner.poll_write(cx, buf) {
            Poll::Ready(Ok(len)) => {
                if let Some(fusewire) = &this.fusewire {
                    fusewire.event(FuseEvent::WriteData(len));
                }
                Poll::Ready(Ok(len))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => {
                if let Some(fusewire) = &this.fusewire {
                    fusewire.event(FuseEvent::Alive);
                }
                Poll::Pending
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = self.project();
        if let Some(fusewire) = &this.fusewire {
            fusewire.event(FuseEvent::Alive);
        }
        this.inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = self.project();
        if let Some(fusewire) = &this.fusewire {
            fusewire.event(FuseEvent::Alive);
        }
        this.inner.poll_shutdown(cx)
    }

    fn poll_write_vectored(self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[IoSlice<'_>]) -> Poll<IoResult<usize>> {
        let this = self.project();
        if let Some(fusewire) = &this.fusewire {
            fusewire.event(FuseEvent::Alive);
        }
        this.inner.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}
