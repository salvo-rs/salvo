mod named_file;
pub use named_file::*;

use bytes::BytesMut;
use futures::Stream;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{
    cmp,
    io::{self, Read, Seek},
};

pub(crate) enum ChunkedState<T> {
    File(Option<T>),
    Future(tokio::task::JoinHandle<Result<(T, BytesMut), io::Error>>),
}

pub struct FileChunk<T> {
    chunk_size: u64,
    read_size: u64,
    buffer_size: u64,
    offset: u64,
    state: ChunkedState<T>,
}

impl<T> Stream for FileChunk<T>
where
    T: Read + Seek + Unpin + Send + 'static,
{
    type Item = Result<BytesMut, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        if self.chunk_size == self.read_size {
            return Poll::Ready(None);
        }

        match self.state {
            ChunkedState::File(ref mut file) => {
                let mut file = file.take().expect("ChunkedReadFile polled after completion");
                let max_bytes = cmp::min(self.chunk_size.saturating_sub(self.read_size), self.buffer_size) as usize;
                let offset = self.offset;
                let fut = tokio::task::spawn_blocking(move || {
                    let mut buf = BytesMut::with_capacity(max_bytes);
                    // safety: it has max bytes capacity, and we don't read it
                    unsafe {
                        buf.set_len(max_bytes);
                    }
                    file.seek(io::SeekFrom::Start(offset))?;

                    file.by_ref().read_exact(&mut buf)?;

                    Ok((file, buf))
                });

                self.state = ChunkedState::Future(fut);
                self.poll_next(cx)
            }
            ChunkedState::Future(ref mut fut) => {
                let (file, buf) = futures::ready!(Pin::new(fut).poll(cx))
                    .map_err(|_| io::Error::new(io::ErrorKind::Other, "BlockingErr"))??;
                self.state = ChunkedState::File(Some(file));

                self.offset += buf.len() as u64;
                self.read_size += buf.len() as u64;

                Poll::Ready(Some(Ok(buf)))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::{ChunkedState, FileChunk};
    use futures::stream::StreamExt;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_chunk_read() {
        const SIZE: u64 = 1024 * 1024 * 5;
        let mock = Cursor::new((0..SIZE).map(|_| rand::random::<u8>()).collect::<Vec<_>>());

        let mut chunk = FileChunk {
            chunk_size: SIZE,
            read_size: 0,
            buffer_size: 65535,
            offset: 0,
            state: ChunkedState::File(Some(mock.clone())),
        };

        let mut result = bytes::BytesMut::with_capacity(SIZE as usize);

        while let Some(Ok(read_chunck)) = chunk.next().await {
            result.extend_from_slice(&read_chunck)
        }

        assert_eq!(mock.into_inner(), result)
    }
}
