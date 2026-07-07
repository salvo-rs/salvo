use std::fmt::{self, Debug, Formatter};
use std::io::{Error, ErrorKind, IoSlice, Result as IoResult};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::{Instant, Sleep};

use crate::conn::ConnCtrl;
use crate::fuse::{ArcConnObserver, FuseConfig};

#[pin_project]
/// A transport stream with inline idle and stalled-write timeouts.
pub struct StraightStream<C> {
    #[pin]
    inner: C,
    idle_timeout: Option<Duration>,
    idle_sleep: Option<Pin<Box<Sleep>>>,
    write_timeout: Option<Duration>,
    // Allocated lazily on the first stalled write. A connection whose writes never block
    // (the common case for a responsive client) never pays for this timer. `Sleep` is
    // `!Unpin`, so it stays boxed to keep `StraightStream` itself `Unpin` as Hyper requires.
    write_sleep: Option<Pin<Box<Sleep>>>,
    write_pending: bool,
    // Shared with handlers: once a handler upgrades the connection to a long-lived
    // protocol (e.g. WebSocket) and calls `ConnCtrl::relax_timeouts`, the idle and
    // write-stall timers below must stop firing, since silence is then expected.
    conn_ctrl: ConnCtrl,
    // Optional custom monitor for bytes transferred. `None` on the default fast path.
    observer: Option<ArcConnObserver>,
}

impl<C> Debug for StraightStream<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("StraightStream").finish()
    }
}

impl<C> StraightStream<C> {
    /// Wraps a transport stream with the selected fuse configuration.
    ///
    /// `conn_ctrl` is shared with the connection's handlers so an upgrade to a long-lived
    /// protocol can [relax](ConnCtrl::relax_timeouts) the idle and write-stall timers.
    /// `observer`, when set, is notified of the bytes read and written on this transport.
    pub fn new(
        inner: C,
        fuse: Option<FuseConfig>,
        conn_ctrl: ConnCtrl,
        observer: Option<ArcConnObserver>,
    ) -> Self {
        let idle_timeout = fuse.and_then(|f| f.connection_idle_timeout);
        let write_timeout = fuse.and_then(|f| f.write_stall_timeout);
        Self {
            inner,
            idle_timeout,
            idle_sleep: idle_timeout.map(|duration| Box::pin(tokio::time::sleep(duration))),
            write_timeout,
            write_sleep: None,
            write_pending: false,
            conn_ctrl,
            observer,
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
                let read = buf.filled().len() - before;
                if read > 0 {
                    if let (Some(timeout), Some(sleep)) =
                        (*this.idle_timeout, this.idle_sleep.as_mut())
                    {
                        // A read that lands after the idle deadline already elapsed must still
                        // fail, or a peer dribbling one byte just past each deadline would keep
                        // resetting the timer and evade the idle timeout entirely.
                        if !this.conn_ctrl.is_relaxed() && sleep.as_mut().poll(cx).is_ready() {
                            return Poll::Ready(Err(timed_out("connection idle timeout")));
                        }
                        sleep.as_mut().reset(Instant::now() + timeout);
                    }
                    if let Some(observer) = this.observer.as_ref() {
                        observer.on_read(read);
                    }
                }
                Poll::Ready(Ok(()))
            }
            Poll::Pending => {
                if !this.conn_ctrl.is_relaxed()
                    && let Some(sleep) = this.idle_sleep.as_mut()
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
                if written > 0 {
                    if let (Some(timeout), Some(sleep)) =
                        (*this.idle_timeout, this.idle_sleep.as_mut())
                    {
                        // As on the read path, a write that completes after the idle deadline
                        // already elapsed must still fail rather than revive an idle connection.
                        if !this.conn_ctrl.is_relaxed() && sleep.as_mut().poll(cx).is_ready() {
                            return Poll::Ready(Err(timed_out("connection idle timeout")));
                        }
                        sleep.as_mut().reset(Instant::now() + timeout);
                    }
                    if let Some(observer) = this.observer.as_ref() {
                        observer.on_write(written);
                    }
                }
                Poll::Ready(Ok(written))
            }
            Poll::Pending => {
                // A relaxed connection (e.g. an upgraded WebSocket) may legitimately
                // stay silent, so neither the write-stall nor the idle timer applies.
                if this.conn_ctrl.is_relaxed() {
                    return Poll::Pending;
                }
                if !*this.write_pending {
                    *this.write_pending = true;
                    if let Some(timeout) = *this.write_timeout {
                        let deadline = Instant::now() + timeout;
                        match this.write_sleep.as_mut() {
                            Some(sleep) => sleep.as_mut().reset(deadline),
                            None => {
                                *this.write_sleep =
                                    Some(Box::pin(tokio::time::sleep_until(deadline)));
                            }
                        }
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
                if written > 0 {
                    if let (Some(timeout), Some(sleep)) =
                        (*this.idle_timeout, this.idle_sleep.as_mut())
                    {
                        // As on the read path, a write that completes after the idle deadline
                        // already elapsed must still fail rather than revive an idle connection.
                        if !this.conn_ctrl.is_relaxed() && sleep.as_mut().poll(cx).is_ready() {
                            return Poll::Ready(Err(timed_out("connection idle timeout")));
                        }
                        sleep.as_mut().reset(Instant::now() + timeout);
                    }
                    if let Some(observer) = this.observer.as_ref() {
                        observer.on_write(written);
                    }
                }
                Poll::Ready(Ok(written))
            }
            Poll::Pending => {
                // A relaxed connection (e.g. an upgraded WebSocket) may legitimately
                // stay silent, so neither the write-stall nor the idle timer applies.
                if this.conn_ctrl.is_relaxed() {
                    return Poll::Pending;
                }
                if !*this.write_pending {
                    *this.write_pending = true;
                    if let Some(timeout) = *this.write_timeout {
                        let deadline = Instant::now() + timeout;
                        match this.write_sleep.as_mut() {
                            Some(sleep) => sleep.as_mut().reset(deadline),
                            None => {
                                *this.write_sleep =
                                    Some(Box::pin(tokio::time::sleep_until(deadline)));
                            }
                        }
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
        let mut stream = StraightStream::new(client, Some(config), ConnCtrl::new(), None);

        let error = stream.read_u8().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn idle_timeout_fires_even_when_late_data_arrives() {
        let (client, server) = tokio::io::duplex(64);
        let config = FuseConfig {
            connection_idle_timeout: Some(Duration::from_millis(10)),
            ..FuseConfig::disabled()
        };
        let mut stream = StraightStream::new(client, Some(config), ConnCtrl::new(), None);

        // Let the idle deadline lapse with no activity, then have the peer send a byte.
        tokio::time::sleep(Duration::from_millis(40)).await;
        let mut server = server;
        server.write_all(b"x").await.unwrap();

        // The connection was idle past its deadline, so the read must fail rather than accept
        // the late byte and silently reset the timer.
        let error = stream.read_u8().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn idle_timeout_fires_even_when_a_late_write_completes() {
        let (client, _server) = tokio::io::duplex(64);
        let config = FuseConfig {
            connection_idle_timeout: Some(Duration::from_millis(10)),
            ..FuseConfig::disabled()
        };
        let mut stream = StraightStream::new(client, Some(config), ConnCtrl::new(), None);

        // Let the idle deadline lapse, then perform an otherwise-successful write.
        tokio::time::sleep(Duration::from_millis(40)).await;

        // A write must not revive a connection that was already idle past its deadline.
        let error = stream.write_all(b"x").await.unwrap_err();
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
        let mut stream = StraightStream::new(client, Some(config), ConnCtrl::new(), None);
        stream.write_all(b"a").await.unwrap();

        let error = stream.write_all(b"b").await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn relaxed_connection_ignores_idle_timeout() {
        let (client, _server) = tokio::io::duplex(64);
        let config = FuseConfig {
            connection_idle_timeout: Some(Duration::from_millis(10)),
            ..FuseConfig::disabled()
        };
        let conn_ctrl = ConnCtrl::new();
        // A handler that upgraded to a long-lived protocol relaxed the timers.
        conn_ctrl.relax_timeouts();
        let mut stream = StraightStream::new(client, Some(config), conn_ctrl, None);

        // With the idle timer relaxed the read must stay pending, not time out.
        let result = tokio::time::timeout(Duration::from_millis(50), stream.read_u8()).await;
        assert!(result.is_err(), "relaxed idle timeout must not fire");
    }

    #[tokio::test]
    async fn observer_sees_bytes_transferred() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use crate::fuse::ConnObserver;

        #[derive(Default)]
        struct Counter {
            read: AtomicUsize,
            write: AtomicUsize,
        }
        impl ConnObserver for Counter {
            fn on_read(&self, bytes: usize) {
                self.read.fetch_add(bytes, Ordering::Relaxed);
            }
            fn on_write(&self, bytes: usize) {
                self.write.fetch_add(bytes, Ordering::Relaxed);
            }
        }

        let counter = Arc::new(Counter::default());
        let (client, server) = tokio::io::duplex(64);
        let mut stream = StraightStream::new(client, None, ConnCtrl::new(), Some(counter.clone()));

        stream.write_all(b"hello").await.unwrap();
        let mut server = server;
        server.write_all(b"hi").await.unwrap();
        let mut buf = [0u8; 2];
        stream.read_exact(&mut buf).await.unwrap();

        assert_eq!(counter.write.load(Ordering::Relaxed), 5);
        assert_eq!(counter.read.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn observer_can_terminate_the_connection() {
        use std::sync::Arc;

        use crate::fuse::ConnObserver;

        // An observer that aborts the connection once too many bytes have been read —
        // the detect-then-terminate loop the fixed timeouts cannot express.
        struct Guard {
            ctrl: ConnCtrl,
            limit: usize,
        }
        impl ConnObserver for Guard {
            fn on_read(&self, bytes: usize) {
                if bytes >= self.limit {
                    self.ctrl.abort();
                }
            }
        }

        let conn_ctrl = ConnCtrl::new();
        let observer = Arc::new(Guard {
            ctrl: conn_ctrl.clone(),
            limit: 4,
        });
        let (client, server) = tokio::io::duplex(64);
        let mut stream = StraightStream::new(client, None, conn_ctrl.clone(), Some(observer));

        let mut server = server;
        server.write_all(b"flood").await.unwrap();
        let mut buf = [0u8; 5];
        stream.read_exact(&mut buf).await.unwrap();

        assert!(
            conn_ctrl.is_aborted(),
            "observer should have aborted the connection"
        );
    }
}
