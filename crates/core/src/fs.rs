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
mod runtime;

use std::cmp;
use std::fmt::{self, Debug, Formatter};
use std::io::Result as IoResult;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use bytes::Bytes;
use futures_util::future::BoxFuture;
use futures_util::stream::Stream;
pub use named_file::*;
pub use runtime::{ChunkFuture, ChunkRead, FileBackend};

/// Internal state machine for [`ChunkedFile`].
pub(crate) enum ChunkedState<T> {
    /// Holding the file, ready to start the next read operation.
    File(Option<T>),
    /// Waiting for the next asynchronous chunk to complete.
    Future(BoxFuture<'static, IoResult<(T, Bytes)>>),
}

/// A streaming file reader that yields data in configurable chunks.
///
/// `ChunkedFile` implements [`Stream`](futures_util::stream::Stream), yielding
/// [`Bytes`] chunks as the file is read. This allows large files to be served
/// without loading the entire content into memory.
///
/// # How It Works
///
/// 1. Reading is delegated to a runtime-specific [`ChunkRead`] implementation
/// 2. Each read operation yields a chunk of up to `buffer_size` bytes
/// 3. The stream completes when `total_size` bytes have been read
///
/// # Type Parameter
///
/// - `T`: The file type, which must implement [`ChunkRead`]
///
/// # Example
///
/// ```ignore
/// use salvo_core::fs::ChunkedFile;
/// use futures_util::StreamExt;
/// use tokio::fs::File;
///
/// let file = File::open("large_file.bin").await.unwrap();
/// let metadata = file.metadata().await.unwrap();
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

impl<T> ChunkedFile<T>
where
    T: ChunkRead,
{
    /// Create a new [`ChunkedFile`] starting from offset 0.
    #[must_use]
    pub fn new(file: T, total_size: u64, buffer_size: u64) -> Self {
        Self::with_offset(file, total_size, buffer_size, 0)
    }

    #[must_use]
    pub(crate) fn with_offset(file: T, total_size: u64, buffer_size: u64, offset: u64) -> Self {
        Self {
            total_size,
            read_size: 0,
            buffer_size,
            offset,
            state: ChunkedState::File(Some(file)),
        }
    }
}

impl<T> Stream for ChunkedFile<T>
where
    T: ChunkRead + Unpin,
{
    type Item = IoResult<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.total_size == this.read_size {
            return Poll::Ready(None);
        }

        match &mut this.state {
            ChunkedState::File(file) => {
                let file = file.take().expect("`ChunkedFile` polled after completion");
                let max_bytes = cmp::min(
                    this.total_size.saturating_sub(this.read_size),
                    this.buffer_size,
                ) as usize;
                let offset = this.offset;
                this.state = ChunkedState::Future(file.read_chunk(offset, max_bytes));
                Pin::new(this).poll_next(cx)
            }
            ChunkedState::Future(fut) => {
                let (file, bytes) = ready!(fut.as_mut().poll(cx))?;
                this.state = ChunkedState::File(Some(file));

                this.offset += bytes.len() as u64;
                this.read_size += bytes.len() as u64;

                Poll::Ready(Some(Ok(bytes)))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::{Cursor, ErrorKind, Read, Seek};
    use std::path::Path;
    use std::str::FromStr;

    use bytes::BytesMut;
    use futures_util::stream::StreamExt;
    use mime::Mime;

    use super::*;
    use crate::http::header::HeaderValue;

    impl ChunkRead for Cursor<Vec<u8>> {
        fn read_chunk(self, offset: u64, max_bytes: usize) -> ChunkFuture<Self> {
            Box::pin(async move {
                let mut file = self;
                let mut buf = Vec::with_capacity(max_bytes);
                file.seek(std::io::SeekFrom::Start(offset))?;
                let bytes = file.by_ref().take(max_bytes as u64).read_to_end(&mut buf)?;
                if bytes == 0 {
                    return Err(ErrorKind::UnexpectedEof.into());
                }
                Ok((file, Bytes::from(buf)))
            })
        }
    }

    #[tokio::test]
    async fn test_chunk_read() {
        const SIZE: u64 = 1024 * 1024 * 5;
        let mock = Cursor::new((0..SIZE).map(|_| fastrand::u8(..)).collect::<Vec<_>>());

        let mut chunk = ChunkedFile::new(mock.clone(), SIZE, 65535);

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
            .backend(FileBackend::Tokio)
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

    #[cfg(all(feature = "io-uring", target_os = "linux"))]
    #[tokio::test]
    async fn test_named_file_builder_io_uring_backend() {
        let src = "Cargo.toml";
        let file = NamedFile::builder(src)
            .backend(FileBackend::IoUring)
            .build()
            .await
            .unwrap();
        assert_eq!(file.path(), Path::new(src));
    }
}
