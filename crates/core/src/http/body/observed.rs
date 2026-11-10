use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;

use super::{Body, Frame, ResBody, SizeHint};
use crate::BoxedError;

/// How production of an observed response body ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyOutcome {
    /// The body reached its end without a source error.
    Complete,
    /// The body source returned an error.
    Error,
    /// The body was dropped before reaching its end (including response replacement or HEAD).
    Dropped,
}

/// Measurements of response-body production, not transport delivery or acknowledgement.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct BodyObservation {
    /// Data bytes yielded to the body's consumer. Trailer bytes are excluded.
    pub bytes_yielded: u64,
    /// Time since observation was attached, including time waiting for consumer polls.
    pub elapsed: Duration,
    /// How body production ended.
    pub outcome: BodyOutcome,
}

pub(super) struct ObservedBody {
    inner: ResBody,
    started: Instant,
    bytes: u64,
    observer: Option<Box<dyn FnOnce(BodyObservation) + Send + Sync>>,
    ended: bool,
}

impl ResBody {
    /// Observe body production once, on completion, a source error, or early drop.
    ///
    /// Attach this after transformations whose output you want to measure. The callback
    /// runs synchronously and must not panic. Dropping an unconsumed HEAD/replacement body
    /// reports `Dropped`; this does not necessarily mean a network failure. Empty bodies
    /// complete immediately, and deferred error bodies retain their catcher semantics.
    ///
    /// Bytes count data yielded to the consumer, **not** bytes written or acknowledged on
    /// a socket. Elapsed time ends at body production/drop, not network completion. Upgraded
    /// protocols have their own lifetime and are not measured by the HTTP response body.
    ///
    /// ```
    /// use salvo_core::http::body::ResBody;
    /// let body = ResBody::from("hello").observe(|event| {
    ///     // Export these as body-production metrics, not socket delivery metrics.
    ///     println!(
    ///         "{:?}: {} bytes in {:?}",
    ///         event.outcome, event.bytes_yielded, event.elapsed
    ///     );
    /// });
    /// drop(body); // Reports Dropped because its bytes were never consumed.
    /// ```
    #[must_use]
    pub fn observe(self, observer: impl FnOnce(BodyObservation) + Send + Sync + 'static) -> Self {
        if self.is_error() || self.is_end_stream() {
            observer(BodyObservation {
                bytes_yielded: 0,
                elapsed: Duration::ZERO,
                outcome: if self.is_error() {
                    BodyOutcome::Error
                } else {
                    BodyOutcome::Complete
                },
            });
            return self;
        }
        Self::Boxed(Box::pin(ObservedBody {
            inner: self,
            started: Instant::now(),
            bytes: 0,
            observer: Some(Box::new(observer)),
            ended: false,
        }))
    }
}

impl ObservedBody {
    fn finish(&mut self, outcome: BodyOutcome) {
        self.ended = true;
        if let Some(observer) = self.observer.take() {
            observer(BodyObservation {
                bytes_yielded: self.bytes,
                elapsed: self.started.elapsed(),
                outcome,
            });
        }
    }
}

impl Drop for ObservedBody {
    fn drop(&mut self) {
        if !self.ended {
            self.finish(BodyOutcome::Dropped);
        }
    }
}

impl Body for ObservedBody {
    type Data = Bytes;
    type Error = BoxedError;
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, BoxedError>>> {
        let this = self.get_mut();
        if this.ended {
            return Poll::Ready(None);
        }
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    this.bytes = this.bytes.saturating_add(data.len() as u64);
                }
                if this.inner.is_end_stream() {
                    this.finish(BodyOutcome::Complete);
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(error))) => {
                this.finish(BodyOutcome::Error);
                Poll::Ready(Some(Err(Box::new(error))))
            }
            Poll::Ready(None) => {
                this.finish(BodyOutcome::Complete);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
    fn is_end_stream(&self) -> bool {
        self.ended || self.inner.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        if self.ended {
            SizeHint::with_exact(0)
        } else {
            self.inner.size_hint()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use http_body_util::BodyExt;
    use sync_wrapper::SyncWrapper;

    use super::*;
    use crate::http::body::BytesFrame;
    use crate::http::{HeaderMap, StatusError};

    fn attach(body: ResBody) -> (ResBody, Arc<Mutex<Vec<BodyObservation>>>) {
        let observations = Arc::new(Mutex::new(Vec::new()));
        let sink = observations.clone();
        (
            body.observe(move |event| sink.lock().unwrap().push(event)),
            observations,
        )
    }

    #[tokio::test]
    async fn completed_body_counts_data_preserves_trailers_and_observes_once() {
        let mut trailers = HeaderMap::new();
        trailers.insert("x-finish", "yes".parse().unwrap());
        let frames: Vec<Result<BytesFrame, BoxedError>> = vec![
            Ok(BytesFrame::data("ab")),
            Ok(BytesFrame(Frame::trailers(trailers.clone()))),
        ];
        let (mut body, events) = attach(ResBody::Stream(SyncWrapper::new(Box::pin(
            futures_util::stream::iter(frames),
        ))));
        assert_eq!(
            body.frame().await.unwrap().unwrap().into_data().unwrap(),
            "ab"
        );
        assert_eq!(
            body.frame()
                .await
                .unwrap()
                .unwrap()
                .into_trailers()
                .unwrap(),
            trailers
        );
        assert!(body.frame().await.is_none());
        assert!(body.frame().await.is_none());
        drop(body);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].bytes_yielded, 2);
        assert_eq!(events[0].outcome, BodyOutcome::Complete);
    }

    #[tokio::test]
    async fn source_errors_end_observation_without_counting_unsent_frames() {
        let frames: Vec<Result<BytesFrame, BoxedError>> = vec![
            Ok(BytesFrame::data("ab")),
            Err(std::io::Error::other("source failed").into()),
            Ok(BytesFrame::data("later")),
        ];
        let (mut body, events) = attach(ResBody::Stream(SyncWrapper::new(Box::pin(
            futures_util::stream::iter(frames),
        ))));
        assert!(body.frame().await.unwrap().is_ok());
        assert!(body.frame().await.unwrap().is_err());
        assert!(body.frame().await.is_none());
        drop(body);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].bytes_yielded, 2);
        assert_eq!(events[0].outcome, BodyOutcome::Error);
    }

    #[tokio::test]
    async fn early_drop_reports_only_the_bytes_already_yielded() {
        let (mut body, events) = attach(ResBody::Chunks(VecDeque::from([
            Bytes::from_static(b"ab"),
            Bytes::from_static(b"not yielded"),
        ])));
        assert!(body.frame().await.unwrap().is_ok());
        drop(body);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].bytes_yielded, 2);
        assert_eq!(events[0].outcome, BodyOutcome::Dropped);
    }

    #[tokio::test]
    async fn buffered_empty_and_deferred_error_bodies_keep_their_semantics() {
        let (mut body, events) = attach(ResBody::from("hello"));
        assert_eq!(body.size_hint().exact(), Some(5));
        assert_eq!(
            body.frame().await.unwrap().unwrap().into_data().unwrap(),
            "hello"
        );
        assert!(body.is_end_stream());
        assert_eq!(events.lock().unwrap()[0].outcome, BodyOutcome::Complete);
        let (body, events) = attach(ResBody::None);
        assert!(body.is_none());
        assert_eq!(events.lock().unwrap()[0].outcome, BodyOutcome::Complete);
        let (body, events) = attach(ResBody::Error(StatusError::bad_request()));
        assert!(body.is_error());
        assert_eq!(events.lock().unwrap()[0].outcome, BodyOutcome::Error);
    }
}
