//! Runtime-specific filesystem adapters used by [`super::ChunkedFile`].

use std::io::{self, ErrorKind, SeekFrom};

use bytes::Bytes;
use futures_util::future::BoxFuture;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Boxed future returned by [`ChunkRead`] implementations.
pub type ChunkFuture<T> = BoxFuture<'static, io::Result<(T, Bytes)>>;

/// Async chunk reader abstraction for file-like backends.
///
/// This trait is intentionally narrow: the backend only needs to support
/// positional chunk reads. That keeps the streaming layer reusable for future
/// Linux-only `io_uring` backends without changing response body handling.
pub trait ChunkRead: Sized + Send + 'static {
    /// Read up to `max_bytes` bytes starting from `offset`.
    fn read_chunk(self, offset: u64, max_bytes: usize) -> ChunkFuture<Self>;
}

impl ChunkRead for File {
    fn read_chunk(self, offset: u64, max_bytes: usize) -> ChunkFuture<Self> {
        Box::pin(async move {
            let mut file = self;
            let mut buf = vec![0u8; max_bytes];
            file.seek(SeekFrom::Start(offset)).await?;
            let bytes = file.read(&mut buf).await?;
            if bytes == 0 {
                return Err(ErrorKind::UnexpectedEof.into());
            }
            buf.truncate(bytes);
            Ok((file, Bytes::from(buf)))
        })
    }
}
