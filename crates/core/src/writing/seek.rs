use bytes::{Bytes, BytesMut};
use futures_core::stream::Stream;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncRead;

#[derive(Debug)]
pub struct ReadSeeker<R>(reader: R);

impl<R> ReadSeeker<R>
where
    R: AsyncSeek + AsyncRead,
{
    /// Create a new [`ReadSeeker`] from a reader which implements [`AsyncRead`] and [`AsyncSeek`].
    pub fn new(reader: R) -> Self {
        ReadSeeker(reader)
    }

    /// Convert an [`AsyncRead`] into a [`Stream`] with item type
    /// `Result<Bytes, std::io::Error>`,
    /// with a specific read buffer initial capacity.
    ///
    /// [`AsyncRead`]: tokio::io::AsyncRead
    /// [`Stream`]: futures_core::Stream
    pub fn with_capacity(reader: R, capacity: usize) -> Self {
        ReaderStream {
            reader: Some(reader),
            buf: BytesMut::with_capacity(capacity),
            capacity,
        }
    }
}

#[async_trait]
impl<R> Writer for ReadSeeker
where
R: AsyncSeek + AsyncRead,
{
    #[inline]
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        match self {
            Some(v) => v.render(res),
            None => {
                res.status_code(StatusCode::NOT_FOUND);
            }
        }
    }
}
