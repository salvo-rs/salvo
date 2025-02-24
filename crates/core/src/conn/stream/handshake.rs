use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::{future::BoxFuture, FutureExt};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, Result};
use tokio_util::sync::CancellationToken;

use crate::conn::HttpBuilder;
use crate::fuse::{ArcFusewire, FuseEvent};
use crate::http::HttpConnection;
use crate::service::HyperHandler;

enum State<S> {
    Handshaking(BoxFuture<'static, Result<S>>),
    Ready(S),
    Error,
}

/// Tls stream.
pub struct HandshakeStream<S> {
    state: State<S>,
    fusewire: Option<ArcFusewire>,
}

impl<S> HandshakeStream<S> {
    pub(crate) fn new<F>(handshake: F, fusewire: Option<ArcFusewire>) -> Self
    where
        F: Future<Output = Result<S>> + Send + 'static,
    {
        if let Some(fusewire) = &fusewire {
            fusewire.event(FuseEvent::TlsHandshaking);
        }
        Self {
            state: State::Handshaking(handshake.boxed()),
            fusewire,
        }
    }

    fn set_state_ready(&mut self, stream: S) {
        self.state = State::Ready(stream);
        if let Some(fusewire) = &self.fusewire {
            fusewire.event(FuseEvent::TlsHandshaked);
        }
    }
}
impl<S> HttpConnection for HandshakeStream<S>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    async fn serve(
        self,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        graceful_stop_token: Option<CancellationToken>,
    ) -> IoResult<()> {
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

impl<S> AsyncRead for HandshakeStream<S>
where
    S: AsyncRead + Unpin + Send + 'static,
{
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<Result<()>> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.set_state_ready(s),
                    Poll::Ready(Err(err)) => {
                        this.state = State::Error;
                        return Poll::Ready(Err(err));
                    }
                    Poll::Pending => {
                        if let Some(fusewire) = &self.fusewire {
                            fusewire.event(FuseEvent::Alive);
                        }
                        return Poll::Pending
                    },
                },
                State::Ready(stream) => {
                    let remaining = buf.remaining();
                    return match Pin::new(stream).poll_read(cx, buf) {
                        Poll::Ready(Ok(())) => {
                            if let Some(fusewire) = &self.fusewire {
                                fusewire.event(FuseEvent::ReadData(remaining - buf.remaining()));
                            }
                            Poll::Ready(Ok(()))
                        }
                        Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                        Poll::Pending => {
                            if let Some(fusewire) = &self.fusewire {
                                fusewire.event(FuseEvent::Alive);
                            }
                            Poll::Pending
                        }
                    };
                },
                State::Error => return Poll::Ready(Err(invalid_data_error("poll read invalid data"))),
            }
        }
    }
}

impl<S> AsyncWrite for HandshakeStream<S>
where
    S: AsyncWrite + Unpin + Send + 'static,
{
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.set_state_ready(s),
                    Poll::Ready(Err(err)) => {
                        this.state = State::Error;
                        return Poll::Ready(Err(err));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_write(cx, buf),
                State::Error => return Poll::Ready(Err(invalid_data_error("poll write invalid data"))),
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.set_state_ready(s),
                    Poll::Ready(Err(err)) => {
                        this.state = State::Error;
                        return Poll::Ready(Err(err));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_flush(cx),
                State::Error => return Poll::Ready(Err(invalid_data_error("poll flush invalid data"))),
            }
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                State::Handshaking(fut) => match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(s)) => this.set_state_ready(s),
                    Poll::Ready(Err(err)) => {
                        this.state = State::Error;
                        return Poll::Ready(Err(err));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                State::Ready(stream) => return Pin::new(stream).poll_shutdown(cx),
                State::Error => return Poll::Ready(Err(invalid_data_error("poll shutdown invalid data"))),
            }
        }
    }
}

fn invalid_data_error(msg: &'static str) -> IoError {
    IoError::new(ErrorKind::InvalidData, msg)
}
