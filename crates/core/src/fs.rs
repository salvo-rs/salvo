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
//!     .preload_threshold(262144)
//!     .build()
//!     .await
//!     .unwrap();
//! ```
//!
//! # Chunked Reading
//!
//! Large files are read in chunks to avoid loading the entire file into memory.
//! `NamedFile::builder(...).preload_threshold(...)` controls the separate
//! small-file preload limit.
//! The [`ChunkedFile`] struct implements [`Stream`],
//! yielding [`Bytes`] chunks as they are read.
mod named_file;
use std::cmp;
use std::fmt::{self, Debug, Formatter};
use std::future::Future;
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
    /// A read failed and the error has already been yielded.
    Done,
}

/// A streaming file reader that yields data in configurable chunks.
///
/// `ChunkedFile` implements [`Stream`], yielding
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
/// - `T`: The file type, which must implement [`Read`], [`Seek`], [`Unpin`], and [`Send`]
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
                    let mut buf = vec![0u8; max_bytes];
                    file.seek(io::SeekFrom::Start(offset))?;
                    let bytes = file.read(&mut buf)?;
                    buf.truncate(bytes);
                    if bytes == 0 {
                        return Err(ErrorKind::UnexpectedEof.into());
                    }
                    Ok((file, Bytes::from(buf)))
                });

                self.state = ChunkedState::Future(fut);
                self.poll_next(cx)
            }
            ChunkedState::Future(ref mut fut) => {
                let result = ready!(Pin::new(fut).poll(cx));
                // The completed task cannot be polled again, including when
                // either the task or the file read failed.
                self.state = ChunkedState::Done;
                let (file, bytes) =
                    result.map_err(|_| IoError::other("`ChunkedFile` block error"))??;
                self.state = ChunkedState::File(Some(file));

                self.offset += bytes.len() as u64;
                self.read_size += bytes.len() as u64;

                Poll::Ready(Some(Ok(bytes)))
            }
            ChunkedState::Done => Poll::Ready(None),
        }
    }
}

/// 异步数据源的 chunk 读取闭包。
/// A chunk-reader closure for an async data source.
///
/// 闭包接收 `(offset, chunk_size)`，返回从 `offset` 开始、最多 `chunk_size`
/// 字节的 `Bytes` 块。数据源可以是 R2 对象、远程 HTTP 响应、内存缓冲区或
/// 任何支持按偏移量读取的异步存储。
/// The closure receives `(offset, chunk_size)` and returns a `Bytes` chunk of at
/// most `chunk_size` bytes starting at `offset`. The source may be an R2 object,
/// a remote HTTP response, an in-memory buffer or any async store that supports
/// offset-based reads.
pub type ChunkReader = Box<
    dyn Fn(u64, usize) -> Pin<Box<dyn Future<Output = IoResult<Bytes>> + Send>> + Send + Sync,
>;

/// [`ChunkedStreamReader`] 的内部状态机。
/// Internal state machine for [`ChunkedStreamReader`].
enum ChunkedStreamState {
    /// 空闲，等待发起下一次读取。
    /// Idle, ready to start the next read.
    Idle,
    /// 等待一次异步 chunk 读取完成。
    /// Waiting for an async chunk read to complete.
    Pending(Pin<Box<dyn Future<Output = IoResult<Bytes>> + Send>>),
    /// 读取失败或已结束。
    /// A read failed or the stream has already completed.
    Done,
}

/// 基于异步闭包的流式读取器。
/// A streaming reader driven by an async chunk-reader closure.
///
/// 与 [`ChunkedFile`] 不同，`ChunkedStreamReader` 不依赖本地文件系统，也不使用
/// `tokio::task::spawn_blocking`，因此可在 `wasm32` 等无法访问阻塞线程池的目标上
/// 使用。它通过 [`ChunkReader`] 闭包逐块读取数据，每次只占用一个 chunk 的内存，
/// 适合大文件的流式下载。
/// Unlike [`ChunkedFile`], `ChunkedStreamReader` does not depend on the local file
/// system nor `tokio::task::spawn_blocking`, so it works on targets such as
/// `wasm32` where no blocking thread pool is available. It reads data chunk by
/// chunk through a [`ChunkReader`] closure, keeping only one chunk in memory at a
/// time, which suits streaming downloads of large files.
pub struct ChunkedStreamReader {
    total_size: u64,
    buffer_size: u64,
    offset: u64,
    read_size: u64,
    reader: ChunkReader,
    state: ChunkedStreamState,
}

impl Debug for ChunkedStreamReader {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChunkedStreamReader")
            .field("total_size", &self.total_size)
            .field("buffer_size", &self.buffer_size)
            .field("offset", &self.offset)
            .field("read_size", &self.read_size)
            .finish()
    }
}

impl ChunkedStreamReader {
    /// 从一个异步 chunk 读取闭包构造流式读取器。
    /// Constructs a streaming reader from an async chunk-reader closure.
    ///
    /// # 参数 / Arguments
    ///
    /// - `total_size`：数据源的总字节数。
    ///   The total byte length of the source.
    /// - `buffer_size`：每次读取的最大字节数。
    ///   The maximum number of bytes read per chunk.
    /// - `reader`：接受 `(offset, chunk_size)` 的异步闭包。
    ///   The async closure receiving `(offset, chunk_size)`.
    pub fn new<F>(total_size: u64, buffer_size: u64, reader: F) -> Self
    where
        F: Fn(u64, usize) -> Pin<Box<dyn Future<Output = IoResult<Bytes>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        Self {
            total_size,
            buffer_size,
            offset: 0,
            read_size: 0,
            reader: Box::new(reader),
            state: ChunkedStreamState::Idle,
        }
    }
}

impl Stream for ChunkedStreamReader {
    type Item = IoResult<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.read_size >= self.total_size {
            return Poll::Ready(None);
        }

        loop {
            match &mut self.state {
                ChunkedStreamState::Idle => {
                    let remaining = self.total_size - self.read_size;
                    let size = cmp::min(remaining, self.buffer_size) as usize;
                    let offset = self.offset;
                    let fut = (self.reader)(offset, size);
                    self.state = ChunkedStreamState::Pending(fut);
                }
                ChunkedStreamState::Pending(fut) => match ready!(fut.as_mut().poll(cx)) {
                    Ok(bytes) => {
                        // 空块视为数据源提前结束，避免死循环。
                        // An empty chunk is treated as an early end of stream to
                        // avoid an infinite loop.
                        if bytes.is_empty() {
                            self.state = ChunkedStreamState::Done;
                            return Poll::Ready(None);
                        }
                        self.offset += bytes.len() as u64;
                        self.read_size += bytes.len() as u64;
                        self.state = ChunkedStreamState::Idle;
                        return Poll::Ready(Some(Ok(bytes)));
                    }
                    Err(err) => {
                        self.state = ChunkedStreamState::Done;
                        return Poll::Ready(Some(Err(err)));
                    }
                },
                ChunkedStreamState::Done => return Poll::Ready(None),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;
    use std::path::Path;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

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
    async fn test_chunked_stream_reader() {
        const SIZE: usize = 1024 * 1024 * 5;
        let data = (0..SIZE).map(|_| fastrand::u8(..)).collect::<Vec<_>>();
        let shared = std::sync::Arc::new(data.clone());

        let mut stream = ChunkedStreamReader::new(SIZE as u64, 65535, move |offset, size| {
            let shared = shared.clone();
            Box::pin(async move {
                let start = offset as usize;
                let end = (offset as usize + size).min(shared.len());
                Ok(Bytes::copy_from_slice(&shared[start..end]))
            })
        });

        let mut result = BytesMut::with_capacity(SIZE);
        while let Some(Ok(chunk)) = stream.next().await {
            result.extend_from_slice(&chunk);
        }
        assert_eq!(result, BytesMut::from(&data[..]));
    }

    #[tokio::test]
    async fn test_chunked_stream_reader_streams_large_file_in_bounded_chunks() {
        // 模拟一个 100 MiB 的数据源。闭包只按需生成单个 chunk，全程无需一次性
        // 分配 100 MiB 内存；通过校验每块大小与偏移量衔接来证明是流式顺序读取。
        // Simulates a 100 MiB source. The closure only materialises one chunk at a
        // time, so the whole 100 MiB is never allocated at once; bounded chunk
        // sizes and contiguous offsets prove streaming, sequential reads.
        const TOTAL: u64 = 100 * 1024 * 1024;
        const BUF: u64 = 64 * 1024;

        let max_chunk = Arc::new(AtomicUsize::new(0));
        let expected_offset = Arc::new(AtomicU64::new(0));
        let calls = Arc::new(AtomicU64::new(0));

        let max_chunk_r = max_chunk.clone();
        let expected_offset_r = expected_offset.clone();
        let calls_r = calls.clone();
        let mut stream = ChunkedStreamReader::new(TOTAL, BUF, move |offset, size| {
            // 偏移量必须与上一块末尾严格衔接，证明不是全量加载。
            // The offset must follow the previous chunk exactly, proving reads are
            // sequential rather than a single whole-file load.
            assert_eq!(offset, expected_offset_r.load(Ordering::SeqCst));
            max_chunk_r.fetch_max(size, Ordering::SeqCst);
            expected_offset_r.fetch_add(size as u64, Ordering::SeqCst);
            calls_r.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(Bytes::from(vec![b'x'; size])) })
        });

        let mut total = 0u64;
        let mut chunk_count = 0u64;
        while let Some(Ok(bytes)) = stream.next().await {
            total += bytes.len() as u64;
            chunk_count += 1;
        }

        assert_eq!(total, TOTAL);
        assert_eq!(chunk_count, TOTAL.div_ceil(BUF));
        assert!(max_chunk.load(Ordering::SeqCst) <= BUF as usize);
        assert_eq!(calls.load(Ordering::SeqCst), chunk_count);
        assert_eq!(expected_offset.load(Ordering::SeqCst), TOTAL);
    }

    #[tokio::test]
    async fn test_chunked_stream_reader_empty_chunk_ends_stream() {
        // 闭包返回空块时应正常结束而不是死循环。
        // An empty chunk from the closure must terminate the stream without looping.
        let mut stream =
            ChunkedStreamReader::new(10, 5, |_, _| Box::pin(async { Ok(Bytes::new()) }));
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn blocking_read_panic_yields_one_error_then_eof() {
        struct PanickingReader;

        impl Read for PanickingReader {
            fn read(&mut self, _: &mut [u8]) -> IoResult<usize> {
                panic!("reader failed inside the blocking task");
            }
        }
        impl Seek for PanickingReader {
            fn seek(&mut self, _: io::SeekFrom) -> IoResult<u64> {
                Ok(0)
            }
        }

        let mut chunk = ChunkedFile {
            total_size: 1,
            read_size: 0,
            buffer_size: 1,
            offset: 0,
            state: ChunkedState::File(Some(PanickingReader)),
        };

        assert!(chunk.next().await.unwrap().is_err());
        assert!(chunk.next().await.is_none());
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
        assert_eq!(file.content_type(), &mime::TEXT_HTML_UTF_8);
        assert_eq!(
            file.content_disposition(),
            Some(&HeaderValue::from_static(
                r#"attachment; filename="attach.file""#
            ))
        );
    }
}