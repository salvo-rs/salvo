use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, Result as IoResult};
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_channel::{mpsc, oneshot};
use hyper::HeaderMap;

/// The producer half of a channel-backed response body.
///
/// Create a sender and its corresponding [`ResBody`](super::ResBody) with
/// [`ResBody::channel`](super::ResBody::channel). Sending data waits for the
/// receiver to become ready, providing backpressure to the producing task.
///
/// # Closing the body
///
/// Dropping the sender ends the response body normally once queued data has
/// been consumed. [`BodySender::close`] and [`BodySender::disconnect`] close
/// only the data channel; after calling either one, send trailers or drop the
/// sender so the trailer channel is resolved as well. Call
/// [`BodySender::send_error`] to report a stream error.
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
            .map_err(|e| IoError::other(format!("failed to poll ready: {e}")))
    }

    /// Returns whether the receiving half has been closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.data_tx.is_closed()
    }
    /// Closes the data channel, preventing any new data from being sent.
    ///
    /// This does not close the separate trailer channel. Send trailers or drop
    /// the sender afterward to let the response body finish.
    pub fn close(&mut self) {
        self.data_tx.close_channel();
    }
    /// Disconnects this sender from the data channel.
    ///
    /// The data channel closes when no other senders remain. This does not
    /// resolve the separate trailer channel.
    pub fn disconnect(&mut self) {
        self.data_tx.disconnect();
    }

    async fn ready(&mut self) -> IoResult<()> {
        futures_util::future::poll_fn(|cx| self.poll_ready(cx)).await
    }

    /// Sends a data chunk once the receiver is ready.
    pub async fn send_data(&mut self, chunk: impl Into<Bytes> + Send) -> IoResult<()> {
        self.ready().await?;
        self.data_tx
            .try_send(Ok(chunk.into()))
            .map_err(|e| IoError::other(format!("failed to send data: {e}")))
    }

    /// Sends the response trailer headers.
    ///
    /// Trailers can be sent only once. A second call, or a call after the
    /// receiver has been dropped, returns an error. Sending trailers does not
    /// close the data channel.
    pub async fn send_trailers(&mut self, trailers: HeaderMap) -> IoResult<()> {
        let Some(tx) = self.trailers_tx.take() else {
            return Err(IoError::other("failed to send trailers"));
        };
        tx.send(trailers)
            .map_err(|_| IoError::other("failed to send trailers"))
    }

    /// Ends the data stream with an I/O error.
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
                let data: Bytes = Bytes::copy_from_slice(buf);
                let len = buf.len();
                Poll::Ready(
                    self.data_tx
                        .try_send(Ok(data))
                        .map(|_| len)
                        .map_err(|e| IoError::other(format!("failed to send data: {e}"))),
                )
            }
            Poll::Ready(Err(e)) => {
                Poll::Ready(Err(IoError::other(format!("failed to poll ready: {e}"))))
            }
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
                let data: Bytes = Bytes::copy_from_slice(buf);
                let len = buf.len();
                Poll::Ready(
                    self.data_tx
                        .try_send(Ok(data))
                        .map(|_| len)
                        .map_err(|e| IoError::other(format!("failed to send data: {e}"))),
                )
            }
            Poll::Ready(Err(e)) => {
                Poll::Ready(Err(IoError::other(format!("failed to poll ready: {e}"))))
            }
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

/// The consumer half of a channel-backed response body.
///
/// This type is normally wrapped in [`ResBody::Channel`](super::ResBody::Channel)
/// by [`ResBody::channel`](super::ResBody::channel), rather than constructed or
/// consumed directly.
pub struct BodyReceiver {
    pub(crate) data_rx: mpsc::Receiver<Result<Bytes, IoError>>,
    pub(crate) trailers_rx: oneshot::Receiver<HeaderMap>,
}
impl Debug for BodyReceiver {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("BodyReceiver").finish()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Error as IoError;

    use bytes::Bytes;
    use futures_channel::{mpsc, oneshot};
    use futures_util::StreamExt;
    use hyper::HeaderMap;

    use super::*;

    #[tokio::test]
    async fn test_send_data_and_is_closed() {
        let (tx, mut rx) = mpsc::channel(1);
        let (trailers_tx, _trailers_rx) = oneshot::channel();
        let mut sender = BodySender {
            data_tx: tx,
            trailers_tx: Some(trailers_tx),
        };
        assert!(!sender.is_closed());
        sender.send_data("hello").await.unwrap();
        let got = rx.next().await.unwrap().unwrap();
        assert_eq!(got, Bytes::from("hello"));
        sender.close();
        assert!(sender.is_closed());
    }

    #[tokio::test]
    async fn test_send_trailers() {
        let (tx, _rx) = mpsc::channel(1);
        let (trailers_tx, trailers_rx) = oneshot::channel();
        let mut sender = BodySender {
            data_tx: tx,
            trailers_tx: Some(trailers_tx),
        };
        let mut map = HeaderMap::new();
        map.insert("x-test", "1".parse().unwrap());
        sender.send_trailers(map.clone()).await.unwrap();
        let got = trailers_rx.await.unwrap();
        assert_eq!(got["x-test"], "1");
    }

    #[tokio::test]
    async fn test_send_error() {
        let (tx, mut rx) = mpsc::channel(1);
        let (trailers_tx, _trailers_rx) = oneshot::channel();
        let mut sender = BodySender {
            data_tx: tx,
            trailers_tx: Some(trailers_tx),
        };
        sender.send_error(IoError::other("fail"));
        let got = rx.next().await.unwrap();
        assert!(got.is_err());
    }

    #[tokio::test]
    async fn test_disconnect() {
        let (tx, _rx) = mpsc::channel(1);
        let (trailers_tx, _trailers_rx) = oneshot::channel();
        let mut sender = BodySender {
            data_tx: tx,
            trailers_tx: Some(trailers_tx),
        };
        sender.disconnect();
        assert!(sender.is_closed());
    }
}
