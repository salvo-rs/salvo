use std::pin::Pin;
use futures::TryStream;
use std::task::{Poll, Context};

/// Struct wrapping a stream which allows a chunk to be pushed back to it to be yielded next.
pub(crate) struct PushChunk<S, T> {
    stream: S,
    pushed: Option<T>,
}

impl<S, T> PushChunk<S, T> {
    unsafe_pinned!(stream: S);
    unsafe_unpinned!(pushed: Option<T>);

    pub(crate) fn new(stream: S) -> Self {
        PushChunk {
            stream,
            pushed: None,
        }
    }
}

impl<S: TryStream> PushChunk<S, S::Ok>
where
    S::Ok: BodyChunk,
{
    fn push_chunk(mut self: Pin<&mut Self>, chunk: S::Ok) {
        if let Some(pushed) = self.as_mut().pushed() {
            debug_panic!(
                "pushing excess chunk: \"{}\" already pushed chunk: \"{}\"",
                show_bytes(chunk.as_slice()),
                show_bytes(pushed.as_slice())
            );
        }

        debug_assert!(!chunk.is_empty(), "pushing empty chunk");

        *self.as_mut().pushed() = Some(chunk);
    }
}

impl<S: TryStream> Stream for PushChunk<S, S::Ok> {
    type Item = std::result::Result<S::Ok, S::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        if let Some(pushed) = self.as_mut().pushed().take() {
            return Poll::Ready(Some(Ok(pushed)));
        }

        self.stream().try_poll_next(cx)
    }
}
