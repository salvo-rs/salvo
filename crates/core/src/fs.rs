//! Filesystem utilities for serving files in HTTP responses.
//!
//! This module provides types for efficiently reading and serving files,
//! with support for chunked transfer and range requests.
//!
//! # Key Types
//!
//! - [`NamedFile`]: Represents a file with metadata for HTTP serving
//! - [`ChunkedFile`]: A streaming reader that reads files in configurable chunks
//!
//! # Example
//!
//! Using `NamedFile` to serve a file:
//!
//! ```ignore
//! use salvo_core::fs::NamedFile;
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn serve_file(res: &mut Response) {
//!     let file = NamedFile::open("./static/document.pdf").await.unwrap();
//!     res.send_file(file);
//! }
//! ```
//!
//! Using the builder pattern for more control:
//!
//! ```ignore
//! use salvo_core::fs::NamedFile;
//!
//! let file = NamedFile::builder("./downloads/report.pdf")
//!     .attached_name("monthly-report.pdf")
//!     .buffer_size(65536)
//!     .build()
//!     .await
//!     .unwrap();
//! ```
//!
//! # Chunked Reading
//!
//! Large files are read in chunks to avoid loading the entire file into memory.
//! The [`ChunkedFile`] struct implements [`Stream`](futures_util::stream::Stream),
//! yielding [`Bytes`](bytes::Bytes) chunks as they are read.
mod named_file;
use std::cmp;
use std::fmt::{self, Debug, Formatter};
use std::io::{self, Error as IoError, ErrorKind, Read, Result as IoResult, Seek};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use bytes::Bytes;
use futures_util::stream::Stream;
pub use named_file::*;

/// Internal state machine for [`ChunkedFile`].
pub(crate) enum ChunkedState<T> {
    /// Holding the file, ready to start the next read operation.
    File(Option<T>),
    /// Waiting for a blocking read operation to complete.
    Future(tokio::task::JoinHandle<IoResult<(T, Bytes)>>),
}

/// A streaming file reader that yields data in configurable chunks.
///
/// `ChunkedFile` implements [`Stream`](futures_util::stream::Stream), yielding
/// [`Bytes`] chunks as the file is read. This allows large files to be served
/// without loading the entire content into memory.
///
/// # How It Works
///
/// 1. Reading is performed in a blocking thread pool via `spawn_blocking`
/// 2. Each read operation yields a chunk of up to `buffer_size` bytes
/// 3. The stream completes when `total_size` bytes have been read
///
/// # Type Parameter
///
/// - `T`: The file type, which must implement [`Read`], [`Seek`], [`Unpin`],
///   and [`Send`]
///
/// # Example
///
/// ```ignore
/// use salvo_core::fs::ChunkedFile;
/// use futures_util::StreamExt;
/// use std::fs::File;
///
/// let file = File::open("large_file.bin").unwrap();
/// let metadata = file.metadata().unwrap();
///
/// let mut stream = ChunkedFile::new(file, metadata.len(), 65536);
///
/// while let Some(chunk) = stream.next().await {
///     match chunk {
///         Ok(bytes) => println!("Read {} bytes", bytes.len()),
///         Err(e) => eprintln!("Error: {}", e),
///     }
/// }
/// ```
pub struct ChunkedFile<T> {
    total_size: u64,
    read_size: u64,
    buffer_size: u64,
    offset: u64,
    state: ChunkedState<T>,
}
impl<T> Debug for ChunkedFile<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChunkedFile")
            .field("total_size", &self.total_size)
            .field("read_size", &self.read_size)
            .field("buffer_size", &self.buffer_size)
            .field("offset", &self.offset)
            .finish()
    }
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
                let mut file = file.take().expect("`ChunkedFile` polled after completion");
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
                    .map_err(|_| IoError::other("`ChunkedFile` block error"))??;
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

        while let Some(Ok(read_chunk)) = chunk.next().await {
            result.extend_from_slice(&read_chunk)
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
        assert_eq!(
            file.content_type(),
            &Mime::from_str("text/html; charset=utf8").unwrap()
        );
        assert_eq!(
            file.content_disposition(),
            Some(&HeaderValue::from_static(
                r#"attachment; filename="attach.file""#
            ))
        );
    }
}
