use std::fmt::{self, Debug, Formatter};
use std::io::{Error, ErrorKind, IoSlice, Result as IoResult};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::{Instant, Sleep};

use crate::fuse::FuseConfig;

#[pin_project]
/// A transport stream with inline idle and stalled-write timeouts.
pub struct StraightStream<C> {
    #[pin]
    inner: C,
    idle_timeout: Option<Duration>,
    idle_sleep: Option<Pin<Box<Sleep>>>,
    write_timeout: Option<Duration>,
    write_sleep: Option<Pin<Box<Sleep>>>,
    write_pending: bool,
}

impl<C> Debug for StraightStream<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("StraightStream").finish()
    }
}

impl<C> StraightStream<C> {
    /// Wraps a transport stream with the selected fuse configuration.
    pub fn new(inner: C, fuse: Option<FuseConfig>) -> Self {
        let idle_timeout = fuse.and_then(|f| f.connection_idle_timeout);
        let write_timeout = fuse.and_then(|f| f.write_stall_timeout);
        Self {
            inner,
            idle_timeout,
            idle_sleep: idle_timeout.map(|duration| Box::pin(tokio::time::sleep(duration))),
            write_timeout,
            write_sleep: write_timeout
                .map(|_| Box::pin(tokio::time::sleep(Duration::from_secs(86400 * 365)))),
            write_pending: false,
        }
    }
}

fn timed_out(kind: &'static str) -> Error {
    Error::new(ErrorKind::TimedOut, kind)
}

impl<C: AsyncRead> AsyncRead for StraightStream<C> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<IoResult<()>> {
        let mut this = self.project();
        let before = buf.filled().len();
        match this.inner.as_mut().poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                if buf.filled().len() > before
                    && let (Some(timeout), Some(sleep)) =
                        (*this.idle_timeout, this.idle_sleep.as_mut())
                {
                    sleep.as_mut().reset(Instant::now() + timeout);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Pending => {
                if let Some(sleep) = this.idle_sleep.as_mut()
                    && sleep.as_mut().poll(cx).is_ready()
                {
                    return Poll::Ready(Err(timed_out("connection idle timeout")));
                }
                Poll::Pending
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
        }
    }
}

impl<C: AsyncWrite> AsyncWrite for StraightStream<C> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        let mut this = self.project();
        match this.inner.as_mut().poll_write(cx, buf) {
            Poll::Ready(Ok(written)) => {
                *this.write_pending = false;
                if written > 0
                    && let (Some(timeout), Some(sleep)) =
                        (*this.idle_timeout, this.idle_sleep.as_mut())
                {
                    sleep.as_mut().reset(Instant::now() + timeout);
                }
                Poll::Ready(Ok(written))
            }
            Poll::Pending => {
                if !*this.write_pending {
                    *this.write_pending = true;
                    if let (Some(timeout), Some(sleep)) =
                        (*this.write_timeout, this.write_sleep.as_mut())
                    {
                        sleep.as_mut().reset(Instant::now() + timeout);
                    }
                }
                if let Some(sleep) = this.write_sleep.as_mut()
                    && sleep.as_mut().poll(cx).is_ready()
                {
                    return Poll::Ready(Err(timed_out("connection write stall timeout")));
                }
                if let Some(sleep) = this.idle_sleep.as_mut()
                    && sleep.as_mut().poll(cx).is_ready()
                {
                    return Poll::Ready(Err(timed_out("connection idle timeout")));
                }
                Poll::Pending
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        self.project().inner.poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<IoResult<usize>> {
        let mut this = self.project();
        match this.inner.as_mut().poll_write_vectored(cx, bufs) {
            Poll::Ready(Ok(written)) => {
                *this.write_pending = false;
                if written > 0
                    && let (Some(timeout), Some(sleep)) =
                        (*this.idle_timeout, this.idle_sleep.as_mut())
                {
                    sleep.as_mut().reset(Instant::now() + timeout);
                }
                Poll::Ready(Ok(written))
            }
            Poll::Pending => {
                if !*this.write_pending {
                    *this.write_pending = true;
                    if let (Some(timeout), Some(sleep)) =
                        (*this.write_timeout, this.write_sleep.as_mut())
                    {
                        sleep.as_mut().reset(Instant::now() + timeout);
                    }
                }
                if let Some(sleep) = this.write_sleep.as_mut()
                    && sleep.as_mut().poll(cx).is_ready()
                {
                    return Poll::Ready(Err(timed_out("connection write stall timeout")));
                }
                if let Some(sleep) = this.idle_sleep.as_mut()
                    && sleep.as_mut().poll(cx).is_ready()
                {
                    return Poll::Ready(Err(timed_out("connection idle timeout")));
                }
                Poll::Pending
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
        }
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn idle_timeout_is_enforced_without_a_task() {
        let (client, _server) = tokio::io::duplex(64);
        let config = FuseConfig {
            connection_idle_timeout: Some(Duration::from_millis(10)),
            write_stall_timeout: None,
            ..FuseConfig::disabled()
        };
        let mut stream = StraightStream::new(client, Some(config));

        let error = stream.read_u8().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn pending_write_has_an_independent_timeout() {
        let (client, _server) = tokio::io::duplex(1);
        let config = FuseConfig {
            connection_idle_timeout: None,
            write_stall_timeout: Some(Duration::from_millis(10)),
            ..FuseConfig::disabled()
        };
        let mut stream = StraightStream::new(client, Some(config));
        stream.write_all(b"a").await.unwrap();

        let error = stream.write_all(b"b").await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
    }
}
