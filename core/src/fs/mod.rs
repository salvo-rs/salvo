mod named_file;
pub use named_file::*;

use bytes::BytesMut;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, io};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncSeek},
};

pub struct FileChunk {
    chunk_size: u64,
    read_size: u64,
    buffer_size: u64,
    offset: u64,
    file: File,
}

impl Stream for FileChunk {
    type Item = Result<BytesMut, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        if self.chunk_size == self.read_size {
            return Poll::Ready(None);
        }

        let max_bytes = cmp::min(self.chunk_size.saturating_sub(self.read_size), self.buffer_size) as usize;
        let offset = self.offset;
        Pin::new(&mut self.file).start_seek(io::SeekFrom::Start(offset))?;
        let mut data = BytesMut::with_capacity(max_bytes);
        // safety: it has max bytes capacity, and we don't read it
        unsafe {
            data.set_len(max_bytes);
        }
        // ReadBuf get a &mut [u8], so it cannot expand by itself, it can only read max_bytes at most
        let mut buf = tokio::io::ReadBuf::new(data.as_mut());

        match Pin::new(&mut self.file).poll_read(cx, &mut buf) {
            Poll::Ready(Ok(())) => {
                // we only read this size data from the file
                let filled = max_bytes - buf.remaining();
                if filled == max_bytes {
                    Poll::Ready(Some(Err(std::io::ErrorKind::UnexpectedEof.into())))
                } else {
                    self.offset += filled as u64;
                    self.read_size += filled as u64;
                    data.truncate(filled);
                    Poll::Ready(Some(Ok(data)))
                }
            }
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
        }
    }
}
