use std::fmt;
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_channel::mpsc;
use futures_channel::oneshot;
use hyper::HeaderMap;

/// A sender half created through [`ResBody::channel()`].
///
/// Useful when wanting to stream chunks from another thread.
///
/// ## Body Closing
///
/// Note that the request body will always be closed normally when the sender is dropped (meaning
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

    async fn ready(&mut self) -> IoResult<()> {
        futures_util::future::poll_fn(|cx| self.poll_ready(cx)).await
    }

    /// Send data on data channel when it is ready.
    pub async fn send_data(&mut self, chunk: impl Into<Bytes>) -> IoResult<()> {
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

impl fmt::Debug for BodySender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = f.debug_tuple("BodySender");

        builder.finish()
    }
}

/// A receiver for [`ResBody`]
pub struct BodyReceiver {
    pub(crate) data_rx: mpsc::Receiver<Result<Bytes, IoError>>,
    pub(crate) trailers_rx: oneshot::Receiver<HeaderMap>,
}
