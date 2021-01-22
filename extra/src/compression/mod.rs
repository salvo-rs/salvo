//! Compress the body of a response.

use async_compression::tokio::bufread::{BrotliEncoder, DeflateEncoder, GzipEncoder};
use async_trait::async_trait;
use http::header::HeaderValue;
use hyper::header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE};
use salvo_core::http::ResponseBody;
use salvo_core::prelude::*;
use tokio_stream::{self, StreamExt};
use tokio_util::io::{ReaderStream, StreamReader};

#[derive(Clone, Copy, Debug)]
pub enum CompressionAlgo {
    BR,
    DEFLATE,
    GZIP,
}

impl From<CompressionAlgo> for HeaderValue {
    #[inline]
    fn from(algo: CompressionAlgo) -> Self {
        match algo {
            CompressionAlgo::BR => HeaderValue::from_static("br"),
            CompressionAlgo::DEFLATE => HeaderValue::from_static("deflate"),
            CompressionAlgo::GZIP => HeaderValue::from_static("gzip"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CompressionHandler {
    pub algo: CompressionAlgo,
    pub content_types: Vec<String>,
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
        }
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
        let content_type = res.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or_default();
        if content_type.is_empty() || res.body().is_none() || !self.content_types.iter().any(|c| content_type.starts_with(&**c)) {
            return;
        }
        let body = res.take_body().unwrap();
        if let ResponseBody::Empty = body {
            return;
        }
        let body = body.map(|item| item.map_err(|_| std::io::ErrorKind::Other));
        match self.algo {
            CompressionAlgo::GZIP => {
                let stream = ReaderStream::new(GzipEncoder::new(StreamReader::new(body)));
                res.streaming(stream);
            }
            CompressionAlgo::DEFLATE => {
                let stream = ReaderStream::new(DeflateEncoder::new(StreamReader::new(body)));
                res.streaming(stream);
            }
            CompressionAlgo::BR => {
                let stream = ReaderStream::new(BrotliEncoder::new(StreamReader::new(body)));
                res.streaming(stream);
            }
        }
        res.headers_mut().remove(CONTENT_LENGTH);
        res.headers_mut().append(CONTENT_ENCODING, self.algo.into());
    }
}

/// Create a wrapping filter that compresses the Body of a [`ResponseBody`](salvo_core::http::ResponseBody)
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
///     .and(StaticFile::neww("./README.md"));
/// ```
pub fn gzip() -> CompressionHandler {
    CompressionHandler::new(CompressionAlgo::GZIP)
}

/// Create a wrapping filter that compresses the Body of a [`ResponseBody`](salvo_core::http::ResponseBody)
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
///     .and(StaticFile::neww("./README.md"));
/// use salvo_core::prelude::*;
/// ```
pub fn deflate() -> CompressionHandler {
    CompressionHandler::new(CompressionAlgo::DEFLATE)
}

/// Create a wrapping filter that compresses the Body of a [`ResponseBody`](salvo_core::http::ResponseBody)
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
///     .and(StaticFile::neww("./README.md"));
/// ```
pub fn brotli() -> CompressionHandler {
    CompressionHandler::new(CompressionAlgo::BR)
}
