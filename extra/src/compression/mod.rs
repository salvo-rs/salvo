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
            CompressionAlgo::Gzip => HeaderValue::from_static("gzip"),
            CompressionAlgo::Deflate => HeaderValue::from_static("deflate"),
            CompressionAlgo::Br => HeaderValue::from_static("br"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct HandlerBuilder {
    algo: CompressionAlgo,
    content_types: Vec<String>,
    min_length: usize,
}
impl Default for HandlerBuilder {
    #[inline]
    fn default() -> Self {
        HandlerBuilder {
            algo: CompressionAlgo::Gzip,
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
}

impl HandlerBuilder {
    #[inline]
    pub fn algo(mut self, algo: CompressionAlgo) -> Self {
        self.algo = algo;
        self
    }
    #[inline]
    pub fn content_types(mut self, content_types: &[String]) -> Self {
        self.content_types = content_types.to_vec();
        self
    }
    #[inline]
    pub fn min_length(mut self, min_length: usize) -> Self {
        self.min_length = min_length;
        self
    }
    #[inline]
    pub fn build(self) -> CompressionHandler {
        let Self {
            algo,
            content_types,
            min_length,
        } = self;
        CompressionHandler {
            algo,
            content_types,
            min_length,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CompressionHandler {
    algo: CompressionAlgo,
    content_types: Vec<String>,
    min_length: usize,
}

impl CompressionHandler {
    #[inline]
    pub fn new(algo: CompressionAlgo) -> Self {
        HandlerBuilder::default().algo(algo).build()
    }
    #[inline]
    pub fn builder() -> HandlerBuilder {
        HandlerBuilder::default()
    }

    #[inline]
    pub fn min_length(&mut self) -> usize {
        self.min_length
    }
    // Set minimum compression size, if body less than this value, no compression
    // default is 1kb
    #[inline]
    pub fn set_min_length(&mut self, size: usize) {
        self.min_length = size;
    }

    #[inline]
    pub fn content_types(&self) -> &Vec<String> {
        &self.content_types
    }
    #[inline]
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

#[cfg(test)]
mod tests {
    use salvo_core::hyper;
    use salvo_core::prelude::*;

    use super::*;

    #[fn_handler]
    async fn hello() -> &'static str {
        "hello"
    }

    #[tokio::test]
    async fn test_gzip() {
        let comp_handler = CompressionHandler::builder()
            .algo(CompressionAlgo::Gzip)
            .min_length(1)
            .build();
        let router = Router::with_after(comp_handler).push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello");
        let request = Request::from_hyper(request.body(hyper::Body::empty()).unwrap());
        let mut response = service.handle(request).await;
        assert_eq!(response.headers().get("content-encoding").unwrap(), "gzip");
        let content = response.take_text().await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_brotli() {
        let router = Router::with_after(brotli()).push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello");
        let request = Request::from_hyper(request.body(hyper::Body::empty()).unwrap());
        let mut response = service.handle(request).await;
        assert_eq!(response.headers().get("content-encoding").unwrap(), "br");
        let content = response.take_text().await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_deflate() {
        let router = Router::with_after(deflate()).push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello");
        let request = Request::from_hyper(request.body(hyper::Body::empty()).unwrap());
        let mut response = service.handle(request).await;
        assert_eq!(response.headers().get("content-encoding").unwrap(), "deflate");
        let content = response.take_text().await.unwrap();
        assert_eq!(content, "hello");
    }
}
