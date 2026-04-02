//! Runtime-specific filesystem adapters used by [`super::ChunkedFile`].

use std::io::{self, ErrorKind, SeekFrom};
use std::path::Path;

use bytes::Bytes;
use futures_util::future::BoxFuture;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Filesystem backend used for async file IO.
///
/// The default backend uses Tokio's file APIs. Future Linux-only `io_uring`
/// support will extend this enum without changing higher-level file-serving
/// builders.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum FileBackend {
    /// Tokio-based async file operations.
    #[default]
    Tokio,
}

impl FileBackend {
    pub(crate) async fn open(self, path: &Path) -> io::Result<OpenedFile> {
        match self {
            Self::Tokio => {
                let file = File::open(path).await?;
                let reader = RuntimeFile::Tokio(file.try_clone().await?);
                Ok(OpenedFile { file, reader })
            }
        }
    }
}

/// Opened file handles used by higher-level HTTP file serving.
pub(crate) struct OpenedFile {
    pub(crate) file: File,
    pub(crate) reader: RuntimeFile,
}

/// Runtime-specific streaming file handle.
#[derive(Debug)]
pub(crate) enum RuntimeFile {
    Tokio(File),
}

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

impl ChunkRead for RuntimeFile {
    fn read_chunk(self, offset: u64, max_bytes: usize) -> ChunkFuture<Self> {
        match self {
            Self::Tokio(file) => Box::pin(async move {
                let (file, bytes) = file.read_chunk(offset, max_bytes).await?;
                Ok((Self::Tokio(file), bytes))
            }),
        }
    }
}
