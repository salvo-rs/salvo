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

        // must call poll_complete before start_seek, and call poll_complete to confirm seek finished
        // https://docs.rs/tokio/1.4.0/tokio/io/trait.AsyncSeek.html#errors
        futures::ready!(Pin::new(&mut self.file).poll_complete(cx))?;
        Pin::new(&mut self.file).start_seek(io::SeekFrom::Start(offset))?;
        futures::ready!(Pin::new(&mut self.file).poll_complete(cx))?;

        let mut data = BytesMut::with_capacity(max_bytes);
        // safety: it has max bytes capacity, and we don't read it
        unsafe {
            data.set_len(max_bytes);
        }
        // Temporary index
        let mut read_num = 0;

        loop {
            let mut buf = tokio::io::ReadBuf::new(&mut data.as_mut()[read_num..]);
            match Pin::new(&mut self.file).poll_read(cx, &mut buf) {
                Poll::Ready(Ok(())) => {
                    // we only read this size data from the file
                    let filled = buf.filled().len();
                    if filled == 0 {
                        return Poll::Ready(Some(Err(std::io::ErrorKind::UnexpectedEof.into())));
                    } else {
                        self.offset += filled as u64;
                        self.read_size += filled as u64;
                        read_num += filled;
                        // read to end
                        if read_num == max_bytes {
                            return Poll::Ready(Some(Ok(data)));
                        } else {
                            // try read more
                            continue;
                        }
                    }
                }
                Poll::Pending => {
                    // have read some buf, but pending here
                    // so return read these data
                    if read_num != 0 {
                        data.truncate(read_num);
                    } else {
                        return Poll::Pending;
                    }
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e))),
            }
        }
    }
}
