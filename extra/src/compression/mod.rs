// Copyright (c) 2018-2020 Sean McArthur
// Licensed under the MIT license http://opensource.org/licenses/MIT
// port from https://github.com/seanmonstar/warp/blob/master/src/filters/compression.rs
//! Compress the body of a response.

use async_compression::tokio::bufread::{BrotliEncoder, DeflateEncoder, GzipEncoder};
use async_trait::async_trait;
use salvo_core::http::header::{HeaderValue, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE};
use salvo_core::http::response::Body;
use salvo_core::prelude::*;
use tokio_stream::{self, StreamExt};
use tokio_util::io::{ReaderStream, StreamReader};

#[derive(Clone, Copy, Debug)]
pub enum CompressionAlgo {
    Br,
    Deflate,
    Gzip,
}

impl From<CompressionAlgo> for HeaderValue {
    #[inline]
    fn from(algo: CompressionAlgo) -> Self {
        match algo {
            CompressionAlgo::Br => HeaderValue::from_static("br"),
            CompressionAlgo::Deflate => HeaderValue::from_static("deflate"),
            CompressionAlgo::Gzip => HeaderValue::from_static("gzip"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CompressionHandler {
    pub algo: CompressionAlgo,
    pub content_types: Vec<String>,
    min_length: usize,
}

impl CompressionHandler {
    pub fn new(algo: CompressionAlgo) -> Self {
        CompressionHandler {
            algo,
            content_types: vec![
                "text/".into(),
                "application/javascript".into(),
                "application/json".into(),
                "application/xml".into(),
                "application/rss+xml".into(),
                "image/svg+xml".into(),
            ],
            min_length: 1024,
        }
    }
    // Set minimum compression size, if body less than this value, no compression
    // default is 1kb
    pub fn min_length(mut self, size: usize) -> Self {
        self.min_length = size;
        self
    }
    pub fn content_types(&self) -> &Vec<String> {
        &self.content_types
    }
    pub fn content_types_mut(&mut self) -> &mut Vec<String> {
        &mut self.content_types
    }
}

#[async_trait]
impl Handler for CompressionHandler {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        let content_type = res
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if content_type.is_empty()
            || res.body().is_none()
            || !self.content_types.iter().any(|c| content_type.starts_with(&**c))
        {
            return;
        }
        if let Some(body) = res.take_body() {
            match body {
                Body::Empty => {
                    return;
                }
                Body::Bytes(body) => {
                    if body.len() < self.min_length {
                        res.set_body(Some(Body::Bytes(body)));
                        return;
                    }
                    let reader = StreamReader::new(tokio_stream::once(Result::<_, std::io::Error>::Ok(body)));
                    match self.algo {
                        CompressionAlgo::Gzip => {
                            let stream = ReaderStream::new(GzipEncoder::new(reader));
                            res.streaming(stream);
                        }
                        CompressionAlgo::Deflate => {
                            let stream = ReaderStream::new(DeflateEncoder::new(reader));
                            res.streaming(stream);
                        }
                        CompressionAlgo::Br => {
                            let stream = ReaderStream::new(BrotliEncoder::new(reader));
                            res.streaming(stream);
                        }
                    }
                }
                Body::Stream(body) => {
                    let body = body.map(|item| item.map_err(|_| std::io::ErrorKind::Other));
                    let reader = StreamReader::new(body);
                    match self.algo {
                        CompressionAlgo::Gzip => {
                            let stream = ReaderStream::new(GzipEncoder::new(reader));
                            res.streaming(stream);
                        }
                        CompressionAlgo::Deflate => {
                            let stream = ReaderStream::new(DeflateEncoder::new(reader));
                            res.streaming(stream);
                        }
                        CompressionAlgo::Br => {
                            let stream = ReaderStream::new(BrotliEncoder::new(reader));
                            res.streaming(stream);
                        }
                    }
                }
            }
        }
        res.headers_mut().remove(CONTENT_LENGTH);
        res.headers_mut().append(CONTENT_ENCODING, self.algo.into());
    }
}

/// Create a middleware that compresses the [`Body`](salvo_core::http::response::Body)
/// using gzip, adding `content-encoding: gzip` to the Response's [`HeaderMap`](hyper::HeaderMap)
///
/// # Example
///
/// ```
/// use salvo_core::prelude::*;
/// use salvo_extra::compression;
/// use salvo_extra::serve::StaticFile;
///
/// let router = Router::new()
///     .after(compression::gzip())
///     .get(StaticFile::new("./README.md"));
/// ```
pub fn gzip() -> CompressionHandler {
    CompressionHandler::new(CompressionAlgo::Gzip)
}

/// Create a middleware that compresses the [`Body`](salvo_core::http::response::Body)
/// using deflate, adding `content-encoding: deflate` to the Response's [`HeaderMap`](hyper::HeaderMap)
///
/// # Example
///
/// ```
/// use salvo_core::prelude::*;
/// use salvo_extra::compression;
/// use salvo_extra::serve::StaticFile;
///
/// let router = Router::new()
///     .after(compression::deflate())
///     .get(StaticFile::new("./README.md"));
/// ```
pub fn deflate() -> CompressionHandler {
    CompressionHandler::new(CompressionAlgo::Deflate)
}

/// Create a middleware that compresses the [`Body`](salvo_core::http::response::Body)
/// using brotli, adding `content-encoding: br` to the Response's [`HeaderMap`](hyper::HeaderMap)
///
/// # Example
///
/// ```
/// use salvo_core::prelude::*;
/// use salvo_extra::compression;
/// use salvo_extra::serve::StaticFile;
///
/// let router = Router::new()
///     .after(compression::brotli())
///     .get(StaticFile::new("./README.md"));
/// ```
pub fn brotli() -> CompressionHandler {
    CompressionHandler::new(CompressionAlgo::Br)
}
