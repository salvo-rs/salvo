//! Compress the body of a response.

use async_compression::tokio::bufread::{BrotliEncoder, DeflateEncoder, GzipEncoder};
use tokio_stream::{self, StreamExt};
use tokio_util::io::{ReaderStream, StreamReader};

use salvo_core::async_trait;
use salvo_core::http::header::{HeaderValue, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE};
use salvo_core::http::response::Body;
use salvo_core::prelude::*;

/// CompressionAlgo
#[derive(Clone, Copy, Debug)]
pub enum CompressionAlgo {
    /// Brotli
    Brotli,
    /// Deflate
    Deflate,
    /// Gzip
    Gzip,
}

impl From<CompressionAlgo> for HeaderValue {
    #[inline]
    fn from(algo: CompressionAlgo) -> Self {
        match algo {
            CompressionAlgo::Gzip => HeaderValue::from_static("gzip"),
            CompressionAlgo::Deflate => HeaderValue::from_static("deflate"),
            CompressionAlgo::Brotli => HeaderValue::from_static("br"),
        }
    }
}

/// CompressionHandler
#[derive(Clone, Debug)]
pub struct CompressionHandler {
    algo: CompressionAlgo,
    content_types: Vec<String>,
    min_length: usize,
}

impl Default for CompressionHandler {
    #[inline]
    fn default() -> Self {
        Self::new(CompressionAlgo::Gzip)
    }
}

impl CompressionHandler {
    /// Create a new `CompressionHandler`.
    #[inline]
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
    /// Create a new `CompressionHandler` with algo.
    #[inline]
    pub fn with_algo(mut self, algo: CompressionAlgo) -> Self {
        self.algo = algo;
        self
    }

    /// get min_length.
    #[inline]
    pub fn min_length(&mut self) -> usize {
        self.min_length
    }
    /// Set minimum compression size, if body less than this value, no compression
    /// default is 1kb
    #[inline]
    pub fn set_min_length(&mut self, size: usize) {
        self.min_length = size;
    }
    /// Create a new `CompressionHandler` with min_length.
    #[inline]
    pub fn with_min_length(mut self, min_length: usize) -> Self {
        self.min_length = min_length;
        self
    }

    /// Get content type list reference.
    #[inline]
    pub fn content_types(&self) -> &Vec<String> {
        &self.content_types
    }
    /// Get content type list mutable reference.
    #[inline]
    pub fn content_types_mut(&mut self) -> &mut Vec<String> {
        &mut self.content_types
    }
    /// Create a new `CompressionHandler` with content types list.
    #[inline]
    pub fn with_content_types(mut self, content_types: &[String]) -> Self {
        self.content_types = content_types.to_vec();
        self
    }
}

#[async_trait]
impl Handler for CompressionHandler {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        ctrl.call_next(req, depot, res).await;
        if ctrl.is_ceased() {
            return;
        }
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
                        CompressionAlgo::Brotli => {
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
                        CompressionAlgo::Brotli => {
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
///     .hoop(compression::gzip())
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
///     .hoop(compression::deflate())
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
///     .hoop(compression::brotli())
///     .get(StaticFile::new("./README.md"));
/// ```
pub fn brotli() -> CompressionHandler {
    CompressionHandler::new(CompressionAlgo::Brotli)
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
        let comp_handler = gzip().with_min_length(1);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let req: Request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut res = service.handle(req).await;
        assert_eq!(res.headers().get("content-encoding").unwrap(), "gzip");
        let content = res.take_text().await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_brotli() {
        let comp_handler = brotli().with_min_length(1);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let req: Request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut res = service.handle(req).await;
        assert_eq!(res.headers().get("content-encoding").unwrap(), "br");
        let content = res.take_text().await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_deflate() {
        let comp_handler = deflate().with_min_length(1);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello");
        let req: Request = request.body(hyper::Body::empty()).unwrap().into();
        let mut res = service.handle(req).await;
        assert_eq!(res.headers().get("content-encoding").unwrap(), "deflate");
        let content = res.take_text().await.unwrap();
        assert_eq!(content, "hello");
    }
}
