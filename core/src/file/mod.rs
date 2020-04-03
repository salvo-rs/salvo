mod named_file;
pub use named_file::*;

use anyhow::Result;
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
use tokio_threadpool::{blocking, ThreadPool};

pub struct ChunkedReadFile {
    length: u64,
    offset: u64,
    file: Option<File>,
}
impl Stream for ChunkedReadFile {
    type Item = Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        if let Some(ref mut fut) = self.future {
            return match Pin::new(fut).poll(cx) {
                Poll::Ready(Ok((file, bytes))) => {
                    self.future.take();
                    self.file = Some(file);
                    self.offset += bytes.len() as u64;
                    Poll::Ready(Some(Ok(bytes)))
                }
                Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e.into()))),
                Poll::Pending => Poll::Pending,
            };
        }

        let chunk_size = self.chunk_size;
        let offset = self.offset;

        if size == offset {
            Poll::Ready(None)
        } else {
            let mut file = self.file.take().expect("Use after completion");
            self.future = Some(
                blocking(move || {
                    let max_bytes: usize;
                    max_bytes = cmp::min(range_size, chunk_size) as usize;
                    // println!("=========size: {}, offset: {}, chunk_size:{} max_bytes:{}", range_size,offset, chunk_size, max_bytes);
                    let mut buf = Vec::with_capacity(max_bytes);
                    file.seek(io::SeekFrom::Start(offset))?;
                    let nbytes = file.by_ref().take(max_bytes as u64).read_to_end(&mut buf)?;
                    if nbytes == 0 {
                        return Err(std::io::ErrorKind::UnexpectedEof.into());
                    }
                    Ok((file, Bytes::from(buf)))
                })
                .boxed_local(),
            );
            self.poll_next(cx)
        }
    }
}
