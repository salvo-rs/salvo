use std::future::Future;
use std::io::{Error, ErrorKind, Result};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{future::BoxFuture, FutureExt};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

enum State<S> {
    Handshaking(BoxFuture<'static, Result<S>>),
    Ready(S),
    Error,
}

/// A handshake stream for tls.
pub struct TlsConnStream<S> {
    state: State<S>,
}

impl<S> TlsConnStream<S> {
    pub(crate) fn new<F>(handshake: F) -> Self
    where
        F: Future<Output = Result<S>> + Send + 'static,
    {
        Self {
            state: State::Handshaking(handshake.boxed()),
        }
    }
}

impl<S> AsyncRead for TlsConnStream<S>
where
    S: AsyncRead + Unpin + Send + 'static,
{
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<Result<()>> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.state = State::Ready(s),
                    Poll::Ready(Err(e)) => {
                        this.state = State::Error;
                        return Poll::Ready(Err(e));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_read(cx, buf),
                State::Error => return Poll::Ready(Err(Error::new(ErrorKind::InvalidData, "invalid data"))),
            }
        }
    }
}

impl<S> AsyncWrite for TlsConnStream<S>
where
    S: AsyncWrite + Unpin + Send + 'static,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::result::Result<usize, Error>> {
        let this = &mut *self;
        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.state = State::Ready(s),
                    Poll::Ready(Err(e)) => {
                        this.state = State::Error;
                        return Poll::Ready(Err(e));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_write(cx, buf),
                State::Error => return Poll::Ready(Err(Error::new(ErrorKind::InvalidData, "invalid data"))),
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Error>> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.state = State::Ready(s),
                    Poll::Ready(Err(e)) => {
                        this.state = State::Error;
                        return Poll::Ready(Err(e));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_flush(cx),
                State::Error => return Poll::Ready(Err(Error::new(ErrorKind::InvalidData, "invalid data"))),
            }
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Error>> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.state = State::Ready(s),
                    Poll::Ready(Err(e)) => {
                        this.state = State::Error;
                        return Poll::Ready(Err(e));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_shutdown(cx),
                State::Error => return Poll::Ready(Err(Error::new(ErrorKind::InvalidData, "invalid data"))),
            }
        }
    }
}
