use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_channel::{mpsc, oneshot};
use hyper::HeaderMap;

/// A sender half created through [`ResBody::Channel`](super::ResBody::Channel).
///
/// Useful when wanting to stream chunks from another thread.
///
/// ## Body Closing
///
/// **Note**: The request body will always be closed normally when the sender is dropped (meaning
/// that the empty terminating chunk will be sent to the remote). If you desire to close the
/// connection with an incomplete response (e.g. in the case of an error during asynchronous
/// processing), call the [`Sender::abort()`] method to abort the body in an abnormal fashion.
///
/// [`Body::channel()`]: struct.Body.html#method.channel
/// [`Sender::abort()`]: struct.Sender.html#method.abort
#[must_use = "Sender does nothing unless sent on"]
pub struct BodySender {
    pub(crate) data_tx: mpsc::Sender<Result<Bytes, IoError>>,
    pub(crate) trailers_tx: Option<oneshot::Sender<HeaderMap>>,
}
impl BodySender {
    /// Check to see if this `Sender` can send more data.
    pub(crate) fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        self.data_tx
            .poll_ready(cx)
            .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to poll ready: {}", e)))
    }

    /// Returns whether this channel is closed without needing a context.
    pub fn is_closed(&self) -> bool {
        self.data_tx.is_closed()
    }
    /// Closes this channel from the sender side, preventing any new messages.
    pub fn close(&mut self) {
        self.data_tx.close_channel();
    }
    /// Disconnects this sender from the channel, closing it if there are no more senders left.
    pub fn disconnect(&mut self) {
        self.data_tx.disconnect();
    }

    async fn ready(&mut self) -> IoResult<()> {
        futures_util::future::poll_fn(|cx| self.poll_ready(cx)).await
    }

    /// Send data on data channel when it is ready.
    pub async fn send_data(&mut self, chunk: impl Into<Bytes> + Send) -> IoResult<()> {
        self.ready().await?;
        self.data_tx
            .try_send(Ok(chunk.into()))
            .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to send data: {}", e)))
    }

    /// Send trailers on trailers channel.
    pub async fn send_trailers(&mut self, trailers: HeaderMap) -> IoResult<()> {
        let tx = match self.trailers_tx.take() {
            Some(tx) => tx,
            None => return Err(IoError::new(ErrorKind::Other, "failed to send railers")),
        };
        tx.send(trailers)
            .map_err(|_| IoError::new(ErrorKind::Other, "failed to send railers"))
    }

    /// Send error on data channel.
    pub fn send_error(&mut self, err: IoError) {
        let _ = self
            .data_tx
            // clone so the send works even if buffer is full
            .clone()
            .try_send(Err(err));
    }
}

impl futures_util::AsyncWrite for BodySender {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<IoResult<usize>> {
        match self.data_tx.poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                let data: Bytes = Bytes::from(buf.to_vec());
                let len = buf.len();
                Poll::Ready(self.data_tx.try_send(Ok(data)).map(|_| len).map_err(|e| {
                    IoError::new(ErrorKind::Other, format!("failed to send data: {}", e))
                }))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(IoError::new(
                ErrorKind::Other,
                format!("failed to poll ready: {}", e),
            ))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<IoResult<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        if self.data_tx.is_closed() {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}

impl tokio::io::AsyncWrite for BodySender {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<IoResult<usize>> {
        match self.data_tx.poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                let data: Bytes = Bytes::from(buf.to_vec());
                let len = buf.len();
                Poll::Ready(self.data_tx.try_send(Ok(data)).map(|_| len).map_err(|e| {
                    IoError::new(ErrorKind::Other, format!("failed to send data: {}", e))
                }))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(IoError::new(
                ErrorKind::Other,
                format!("failed to poll ready: {}", e),
            ))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<IoResult<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        if self.data_tx.is_closed() {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}

impl Debug for BodySender {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut builder = f.debug_tuple("BodySender");

        builder.finish()
    }
}

/// A receiver for [`ResBody`](super::ResBody).
pub struct BodyReceiver {
    pub(crate) data_rx: mpsc::Receiver<Result<Bytes, IoError>>,
    pub(crate) trailers_rx: oneshot::Receiver<HeaderMap>,
}
