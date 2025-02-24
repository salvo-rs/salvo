//! Filesystem module
mod named_file;
pub use named_file::*;

use std::cmp;
use std::io::{self, Error as IoError, ErrorKind, Read, Result as IoResult, Seek};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use bytes::Bytes;
use futures_util::stream::Stream;

pub(crate) enum ChunkedState<T> {
    File(Option<T>),
    Future(tokio::task::JoinHandle<IoResult<(T, Bytes)>>),
}

/// A stream of bytes that reads a file in chunks.
///
/// This struct is used to read a file in chunks, where each chunk is a `Bytes` object.
/// It implements the `Stream` trait from the `futures_util` crate.
pub struct ChunkedFile<T> {
    total_size: u64,
    read_size: u64,
    buffer_size: u64,
    offset: u64,
    state: ChunkedState<T>,
}

impl<T> Stream for ChunkedFile<T>
where
    T: Read + Seek + Unpin + Send + 'static,
{
    type Item = IoResult<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        if self.total_size == self.read_size {
            return Poll::Ready(None);
        }

        match self.state {
            ChunkedState::File(ref mut file) => {
                let mut file = file
                    .take()
                    .expect("`ChunkedReadFile` polled after completion");
                let max_bytes = cmp::min(
                    self.total_size.saturating_sub(self.read_size),
                    self.buffer_size,
                ) as usize;
                let offset = self.offset;
                let fut = tokio::task::spawn_blocking(move || {
                    let mut buf = Vec::with_capacity(max_bytes);
                    file.seek(io::SeekFrom::Start(offset))?;
                    let bytes = file.by_ref().take(max_bytes as u64).read_to_end(&mut buf)?;
                    if bytes == 0 {
                        return Err(ErrorKind::UnexpectedEof.into());
                    }
                    Ok((file, Bytes::from(buf)))
                });

                self.state = ChunkedState::Future(fut);
                self.poll_next(cx)
            }
            ChunkedState::Future(ref mut fut) => {
                let (file, bytes) = ready!(Pin::new(fut).poll(cx))
                    .map_err(|_| IoError::new(ErrorKind::Other, "BlockingErr"))??;
                self.state = ChunkedState::File(Some(file));

                self.offset += bytes.len() as u64;
                self.read_size += bytes.len() as u64;

                Poll::Ready(Some(Ok(bytes)))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;
    use std::path::Path;
    use std::str::FromStr;

    use bytes::BytesMut;
    use futures_util::stream::StreamExt;
    use mime::Mime;

    use super::*;
    use crate::http::header::HeaderValue;

    #[tokio::test]
    async fn test_chunk_read() {
        const SIZE: u64 = 1024 * 1024 * 5;
        let mock = Cursor::new((0..SIZE).map(|_| fastrand::u8(..)).collect::<Vec<_>>());

        let mut chunk = ChunkedFile {
            total_size: SIZE,
            read_size: 0,
            buffer_size: 65535,
            offset: 0,
            state: ChunkedState::File(Some(mock.clone())),
        };

        let mut result = BytesMut::with_capacity(SIZE as usize);

        while let Some(Ok(read_chunck)) = chunk.next().await {
            result.extend_from_slice(&read_chunck)
        }

        assert_eq!(mock.into_inner(), result)
    }
    #[tokio::test]
    async fn test_named_file_builder() {
        let src = "Cargo.toml";
        // println!("current path: {:?}", std::env::current_dir());
        // println!("current current_exe: {:?}", std::env::current_exe());
        let file = NamedFile::builder(src)
            .attached_name("attach.file")
            .buffer_size(8888)
            .content_type(Mime::from_str("text/html").unwrap())
            .build()
            .await
            .unwrap();
        assert_eq!(file.path(), Path::new(src));
        assert_eq!(file.content_type(), &Mime::from_str("text/html").unwrap());
        assert_eq!(
            file.content_disposition(),
            Some(&HeaderValue::from_static(
                r#"attachment; filename="attach.file""#
            ))
        );
    }
}
