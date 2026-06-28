use std::fmt::{self, Debug, Formatter};
#[cfg(any(feature = "http1", feature = "http2", feature = "quinn", test))]
use std::future::Future;
#[cfg(any(feature = "http1", feature = "http2", feature = "quinn", test))]
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
#[cfg(any(feature = "http1", feature = "http2", feature = "quinn", test))]
use std::task::{Context, Poll};

use futures_util::task::AtomicWaker;

const RUNNING: u8 = 0;
const GRACEFUL_SHUTDOWN: u8 = 1;
const ABORT: u8 = 2;

/// Controls the lifetime of the transport connection serving a request.
///
/// A control is shared by every request multiplexed over the same connection.
/// Shutting down an HTTP/2 or HTTP/3 connection therefore also affects its
/// other in-flight request streams.
#[derive(Clone)]
pub struct ConnCtrl {
    inner: Arc<Inner>,
}

struct Inner {
    state: AtomicU8,
    waker: AtomicWaker,
}

impl Default for ConnCtrl {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for ConnCtrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnCtrl")
            .field("state", &self.state())
            .finish()
    }
}

impl ConnCtrl {
    /// Creates a connection control in the running state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                state: AtomicU8::new(RUNNING),
                waker: AtomicWaker::new(),
            }),
        }
    }

    /// Stops accepting new requests and lets accepted requests finish.
    ///
    /// A later call to [`abort`](Self::abort) escalates graceful shutdown to an
    /// immediate abort.
    pub fn graceful_shutdown(&self) {
        if self
            .inner
            .state
            .compare_exchange(
                RUNNING,
                GRACEFUL_SHUTDOWN,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            self.inner.waker.wake();
        }
    }

    /// Immediately aborts the entire transport connection.
    ///
    /// The current handler may continue until it next yields, but its response
    /// is not guaranteed to be written.
    pub fn abort(&self) {
        if self.inner.state.swap(ABORT, Ordering::AcqRel) != ABORT {
            self.inner.waker.wake();
        }
    }

    /// Returns `true` after graceful shutdown has been requested.
    #[must_use]
    pub fn is_graceful_shutdown(&self) -> bool {
        self.inner.state.load(Ordering::Acquire) == GRACEFUL_SHUTDOWN
    }

    /// Returns `true` after immediate abort has been requested.
    #[must_use]
    pub fn is_aborted(&self) -> bool {
        self.inner.state.load(Ordering::Acquire) == ABORT
    }

    pub(crate) fn state(&self) -> ConnState {
        match self.inner.state.load(Ordering::Acquire) {
            GRACEFUL_SHUTDOWN => ConnState::GracefulShutdown,
            ABORT => ConnState::Abort,
            _ => ConnState::Running,
        }
    }

    #[cfg(any(feature = "http1", feature = "http2", feature = "quinn", test))]
    pub(crate) fn notified(&self) -> Notified<'_> {
        Notified {
            ctrl: self,
            abort_only: false,
        }
    }

    #[cfg(any(feature = "http1", feature = "http2", feature = "quinn", test))]
    pub(crate) fn aborted(&self) -> Notified<'_> {
        Notified {
            ctrl: self,
            abort_only: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnState {
    Running,
    GracefulShutdown,
    Abort,
}

#[cfg(any(feature = "http1", feature = "http2", feature = "quinn", test))]
pub(crate) struct Notified<'a> {
    ctrl: &'a ConnCtrl,
    abort_only: bool,
}

#[cfg(any(feature = "http1", feature = "http2", feature = "quinn", test))]
impl Future for Notified<'_> {
    type Output = ConnState;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let state = self.ctrl.state();
        if state == ConnState::Abort || (!self.abort_only && state == ConnState::GracefulShutdown) {
            return Poll::Ready(state);
        }
        self.ctrl.inner.waker.register(cx.waker());
        let state = self.ctrl.state();
        if state == ConnState::Abort || (!self.abort_only && state == ConnState::GracefulShutdown) {
            Poll::Ready(state)
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn abort_wakes_waiter() {
        let ctrl = ConnCtrl::new();
        let signal = ctrl.clone();
        let waiter = tokio::spawn(async move { signal.notified().await });
        ctrl.abort();
        assert_eq!(waiter.await.unwrap(), ConnState::Abort);
    }

    #[test]
    fn abort_escalates_graceful_shutdown() {
        let ctrl = ConnCtrl::new();
        ctrl.graceful_shutdown();
        assert!(ctrl.is_graceful_shutdown());
        ctrl.abort();
        assert!(ctrl.is_aborted());
    }

    #[tokio::test]
    async fn abort_waiter_ignores_graceful_shutdown() {
        let ctrl = ConnCtrl::new();
        ctrl.graceful_shutdown();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), ctrl.aborted())
                .await
                .is_err()
        );
        ctrl.abort();
        assert_eq!(ctrl.aborted().await, ConnState::Abort);
    }
}
