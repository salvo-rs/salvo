use std::future::Future;
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::{BoxFuture, FutureExt};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::async_trait;
use crate::conn::HttpBuilders;
use crate::http::{HttpConnection, Version};
use crate::service::HyperHandler;

enum State<S> {
    Handshaking(BoxFuture<'static, IoResult<S>>),
    Ready(S),
    Error(IoError),
}

/// A handshake stream for tls.
pub struct TlsConnStream<S> {
    state: State<S>,
}

impl<S> TlsConnStream<S> {
    pub(crate) fn new<F>(handshake: F) -> Self
    where
        F: Future<Output = IoResult<S>> + Send + 'static,
    {
        Self {
            state: State::Handshaking(handshake.boxed()),
        }
    }
}

#[async_trait]
impl<S> HttpConnection for TlsConnStream<S>
where
    S: HttpConnection + Unpin + Send + 'static,
{
    async fn version(&mut self) -> Option<Version> {
        match &mut self.state {
            State::Handshaking(fut) => match fut.await {
                Ok(s) => self.state = State::Ready(s),
                Err(e) => {
                    self.state = State::Error(e);
                    return None;
                }
            },
            State::Ready(_) => {}
            State::Error(_) => {
                return None;
            }
        }
        if let State::Ready(s) = &mut self.state {
            s.version().await
        } else {
            unreachable!()
        }
    }
    async fn serve(mut self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()> {
        match &mut self.state {
            State::Handshaking(fut) => match fut.await {
                Ok(s) => self.state = State::Ready(s),
                Err(e) => {
                    self.state = State::Error(e);
                }
            },
            State::Ready(_) => {}
            State::Error(e) => {
                return Err(IoError::new(ErrorKind::Other, e.to_string()));
            }
        }
        if let State::Ready(s) = self.state {
            s.serve(handler, builders).await
        } else if let State::Error(e) = self.state {
            return Err(IoError::new(ErrorKind::Other, e.to_string()));
        } else {
            unreachable!()
        }
    }
}

impl<S> AsyncRead for TlsConnStream<S>
where
    S: AsyncRead + Unpin + Send + 'static,
{
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<IoResult<()>> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.state = State::Ready(s),
                    Poll::Ready(Err(e)) => {
                        this.state = State::Error(e);
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_read(cx, buf),
                State::Error(e) => return Poll::Ready(Err(IoError::new(ErrorKind::InvalidData, e.to_string()))),
            }
        }
    }
}

impl<S> AsyncWrite for TlsConnStream<S>
where
    S: AsyncWrite + Unpin + Send + 'static,
{
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        let this = &mut *self;
        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.state = State::Ready(s),
                    Poll::Ready(Err(e)) => {
                        this.state = State::Error(e);
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_write(cx, buf),
                State::Error(e) => return Poll::Ready(Err(IoError::new(ErrorKind::InvalidData, e.to_string()))),
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.state = State::Ready(s),
                    Poll::Ready(Err(e)) => {
                        this.state = State::Error(e);
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_flush(cx),
                State::Error(e) => return Poll::Ready(Err(IoError::new(ErrorKind::InvalidData, e.to_string()))),
            }
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.state = State::Ready(s),
                    Poll::Ready(Err(e)) => {
                        this.state = State::Error(e);
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_shutdown(cx),
                State::Error(e) => return Poll::Ready(Err(IoError::new(ErrorKind::InvalidData, e.to_string()))),
            }
        }
    }
}
