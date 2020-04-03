mod named_file;
pub use named_file::*;

use anyhow::Result;
use futures::future::LocalBoxFuture;
use futures::FutureExt;
use futures::Stream;
use hyper::body::Bytes;
use std::cell::RefCell;
use std::fmt::Write;
use std::fs::{DirEntry, File};
use std::future::Future;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::{cmp, io};
use tokio::task;

pub struct FileChunk {
    chunk_size: u64,
    read_size: u64,
    buffer_size: u64,
    offset: u64,
    file: File,
}
impl Stream for FileChunk {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let Self {
            chunk_size,
            ref mut read_size,
            ref mut offset,
            buffer_size,
            ref mut file,
        } = *self;

        if chunk_size == *read_size {
            Poll::Ready(None)
        } else {
            let max_bytes: usize;
            max_bytes = cmp::min(chunk_size.saturating_sub(*read_size), buffer_size) as usize;
            let mut buf = Vec::with_capacity(max_bytes);
            file.seek(io::SeekFrom::Start(*offset))?;
            let nbytes = file.by_ref().take(max_bytes as u64).read_to_end(&mut buf)?;
            if nbytes == 0 {
                return Poll::Ready(Some(Err(std::io::ErrorKind::UnexpectedEof.into())));
            } else {
                *offset += nbytes as u64;
                *read_size += nbytes as u64;
                Poll::Ready(Some(Ok(Bytes::from(buf))))
            }
        }
    }
}
