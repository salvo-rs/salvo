use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, ErrorKind, IoSlice, Result as IoResult};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, Result};
use tokio::time::Sleep;

use crate::fuse::FuseConfig;

enum State<S> {
    Handshaking(BoxFuture<'static, Result<S>>),
    Ready(S),
    Error,
}

/// A lazily handshaken TLS stream with an inline handshake timeout.
pub struct HandshakeStream<S> {
    state: State<S>,
    timeout: Option<Pin<Box<Sleep>>>,
}

impl<S> Debug for HandshakeStream<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandshakeStream").finish()
    }
}

impl<S> HandshakeStream<S> {
    #[doc(hidden)]
    pub fn new<F>(handshake: F, fuse: Option<FuseConfig>) -> Self
    where
        F: Future<Output = Result<S>> + Send + 'static,
    {
        Self {
            state: State::Handshaking(handshake.boxed()),
            timeout: fuse
                .and_then(|f| f.tls_handshake_timeout)
                .map(|duration| Box::pin(tokio::time::sleep(duration))),
        }
    }

    fn poll_handshake(&mut self, cx: &mut Context<'_>) -> Poll<Result<()>>
    where
        S: Unpin,
    {
        match &mut self.state {
            State::Handshaking(future) => match future.poll_unpin(cx) {
                Poll::Ready(Ok(stream)) => {
                    // A handshake that completes after its deadline already elapsed must still
                    // fail; otherwise the timeout only bites when the timer task happens to be
                    // polled first, making it a soft limit under scheduler delay.
                    if let Some(timeout) = &mut self.timeout
                        && timeout.as_mut().poll(cx).is_ready()
                    {
                        self.state = State::Error;
                        return Poll::Ready(Err(IoError::new(
                            ErrorKind::TimedOut,
                            "TLS handshake timeout",
                        )));
                    }
                    self.state = State::Ready(stream);
                    self.timeout = None;
                    Poll::Ready(Ok(()))
                }
                Poll::Ready(Err(error)) => {
                    self.state = State::Error;
                    Poll::Ready(Err(error))
                }
                Poll::Pending => {
                    if let Some(timeout) = &mut self.timeout
                        && timeout.as_mut().poll(cx).is_ready()
                    {
                        self.state = State::Error;
                        return Poll::Ready(Err(IoError::new(
                            ErrorKind::TimedOut,
                            "TLS handshake timeout",
                        )));
                    }
                    Poll::Pending
                }
            },
            State::Ready(_) => Poll::Ready(Ok(())),
            State::Error => Poll::Ready(Err(invalid_data_error("TLS stream is unavailable"))),
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for HandshakeStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        loop {
            match self.poll_handshake(cx) {
                Poll::Ready(Ok(())) => {
                    if let State::Ready(stream) = &mut self.state {
                        return Pin::new(stream).poll_read(cx, buf);
                    }
                }
                other => return other,
            }
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for HandshakeStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<IoResult<usize>> {
        match self.poll_handshake(cx) {
            Poll::Ready(Ok(())) => match &mut self.state {
                State::Ready(stream) => Pin::new(stream).poll_write(cx, buf),
                _ => unreachable!(),
            },
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        match self.poll_handshake(cx) {
            Poll::Ready(Ok(())) => match &mut self.state {
                State::Ready(stream) => Pin::new(stream).poll_flush(cx),
                _ => unreachable!(),
            },
            other => other,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        match self.poll_handshake(cx) {
            Poll::Ready(Ok(())) => match &mut self.state {
                State::Ready(stream) => Pin::new(stream).poll_shutdown(cx),
                _ => unreachable!(),
            },
            other => other,
        }
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<IoResult<usize>> {
        match self.poll_handshake(cx) {
            Poll::Ready(Ok(())) => match &mut self.state {
                State::Ready(stream) => Pin::new(stream).poll_write_vectored(cx, bufs),
                _ => unreachable!(),
            },
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_write_vectored(&self) -> bool {
        matches!(&self.state, State::Ready(stream) if stream.is_write_vectored())
    }
}

fn invalid_data_error(message: &'static str) -> IoError {
    IoError::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, DuplexStream};

    use super::*;

    #[tokio::test]
    async fn handshake_timeout_is_enforced_without_a_task() {
        let config = FuseConfig {
            tls_handshake_timeout: Some(Duration::from_millis(10)),
            ..FuseConfig::disabled()
        };
        let handshake = pending::<IoResult<DuplexStream>>();
        let mut stream = HandshakeStream::new(handshake, Some(config));

        let error = stream.read_u8().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn handshake_completing_after_the_deadline_is_rejected() {
        let config = FuseConfig {
            tls_handshake_timeout: Some(Duration::from_millis(10)),
            ..FuseConfig::disabled()
        };
        let (server, _client) = tokio::io::duplex(64);
        // A handshake that is already complete the first time it is polled.
        let handshake = std::future::ready(Ok::<_, std::io::Error>(server));
        let mut stream = HandshakeStream::new(handshake, Some(config));

        // Let the handshake deadline lapse before the stream is ever polled.
        tokio::time::sleep(Duration::from_millis(40)).await;

        // The handshake resolves immediately now, but its deadline already passed, so it must
        // be rejected rather than accepted.
        let error = tokio::time::timeout(Duration::from_secs(1), stream.read_u8())
            .await
            .expect("handshake read should resolve, not hang")
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
    }
}
