//! Compress the body of a response.
use std::io::{Cursor, Error as IoError, ErrorKind};
use std::str::FromStr;

use async_compression::tokio::bufread::{BrotliEncoder, DeflateEncoder, GzipEncoder};
use bytes::BytesMut;
use tokio::io::AsyncReadExt;
use tokio_stream::{self, StreamExt};
use tokio_util::io::{ReaderStream, StreamReader};

use salvo_core::http::header::{HeaderValue, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE};
use salvo_core::http::ResBody;
use salvo_core::{async_trait, Depot, Handler, Request, Response, FlowCtrl};

/// CompressionAlgo
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
#[non_exhaustive]
pub enum CompressionAlgo {
    /// Gzip
    Gzip,
    /// Deflate
    Deflate,
    /// Brotli
    Brotli,
}

impl FromStr for CompressionAlgo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "br" => Ok(CompressionAlgo::Brotli),
            "gzip" => Ok(CompressionAlgo::Gzip),
            "deflate" => Ok(CompressionAlgo::Deflate),
            _ => Err(format!("unknown compression algorithm: {}", s)),
        }
    }
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

/// Compression
#[derive(Clone, Debug)]
pub struct Compression {
    algos: Vec<CompressionAlgo>,
    content_types: Vec<String>,
    min_length: usize,
    force_priority: bool,
}

impl Default for Compression {
    #[inline]
    fn default() -> Self {
        Self {
            algos: [CompressionAlgo::Brotli, CompressionAlgo::Gzip, CompressionAlgo::Deflate]
                .into_iter()
                .collect(),
            content_types: vec![
                "text/".into(),
                "application/javascript".into(),
                "application/json".into(),
                "application/xml".into(),
                "application/rss+xml".into(),
                "image/svg+xml".into(),
            ],
            min_length: 1024,
            force_priority: false,
        }
    }
}

impl Compression {
    /// Create a new `Compression`.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets `Compression` with algos.
    #[inline]
    pub fn with_algos(mut self, algos: &[CompressionAlgo]) -> Self {
        self.algos = algos.to_vec();
        self
    }

    /// get min_length.
    #[inline]
    pub fn min_length(&mut self) -> usize {
        self.min_length
    }
    /// Sets minimum compression size, if body less than this value, no compression
    /// default is 1kb
    #[inline]
    pub fn set_min_length(&mut self, size: usize) {
        self.min_length = size;
    }
    /// Sets `Compression` with min_length.
    #[inline]
    pub fn with_min_length(mut self, min_length: usize) -> Self {
        self.min_length = min_length;
        self
    }
    /// Sets `Compression` with force_priority.
    #[inline]
    pub fn with_force_priority(mut self, force_priority: bool) -> Self {
        self.force_priority = force_priority;
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
    /// Sets `Compression` with content types list.
    #[inline]
    pub fn with_content_types(mut self, content_types: &[String]) -> Self {
        self.content_types = content_types.to_vec();
        self
    }

    fn negotiate(&self, header: &str) -> Option<CompressionAlgo> {
        let accept_algos = parse_accept_encoding(header);
        if self.force_priority {
            let accept_algos = accept_algos.into_iter().map(|(algo, _)| algo).collect::<Vec<_>>();
            self.algos.iter().find(|algo| accept_algos.contains(algo)).copied()
        } else {
            accept_algos
                .into_iter()
                .find_map(|(algo, _)| if self.algos.contains(&algo) { Some(algo) } else { None })
        }
    }
}

fn parse_accept_encoding(header: &str) -> Vec<(CompressionAlgo, u8)> {
    let mut vec = header
        .split(',')
        .filter_map(|s| {
            let mut iter = s.trim().split(';');
            let (algo, q) = (iter.next()?, iter.next());
            let algo = algo.trim().parse().ok()?;
            let q = q
                .and_then(|q| {
                    q.trim()
                        .strip_prefix("q=")
                        .and_then(|q| q.parse::<f32>().map(|f| (f * 100.0) as u8).ok())
                })
                .unwrap_or(100u8);
            Some((algo, q))
        })
        .collect::<Vec<(CompressionAlgo, u8)>>();

    vec.sort_by(|(_, a), (_, b)| match b.cmp(a) {
        std::cmp::Ordering::Equal => std::cmp::Ordering::Greater,
        other => other,
    });

    vec
}

async fn compress_bytes(algo: CompressionAlgo, bytes: &[u8]) -> Result<Vec<u8>, IoError> {
    let mut data = vec![];
    match algo {
        CompressionAlgo::Gzip => {
            GzipEncoder::new(Cursor::new(bytes)).read_to_end(&mut data).await?;
        }
        CompressionAlgo::Deflate => {
            DeflateEncoder::new(Cursor::new(bytes)).read_to_end(&mut data).await?;
        }
        CompressionAlgo::Brotli => {
            BrotliEncoder::new(Cursor::new(bytes)).read_to_end(&mut data).await?;
        }
    };
    Ok(data)
}

#[async_trait]
impl Handler for Compression {
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

        let algo = if let Some(algo) = req
            .headers()
            .get(ACCEPT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| self.negotiate(v))
        {
            algo
        } else {
            return;
        };

        match res.take_body() {
            ResBody::None => {
                return;
            }
            ResBody::Once(bytes) => {
                if bytes.len() < self.min_length {
                    res.set_body(ResBody::Once(bytes));
                    return;
                }
                match compress_bytes(algo, &bytes).await {
                    Ok(data) => {
                        res.set_body(ResBody::Once(data.into()));
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "compression failed");
                        res.set_body(ResBody::Once(bytes));
                        return;
                    }
                }
            }
            ResBody::Chunks(chunks) => {
                let len = chunks.iter().map(|c| c.len()).sum();
                if len < self.min_length {
                    res.set_body(ResBody::Chunks(chunks));
                    return;
                }
                let mut bytes = BytesMut::with_capacity(len);
                for chunk in &chunks {
                    bytes.extend_from_slice(chunk);
                }
                match compress_bytes(algo, &bytes).await {
                    Ok(data) => {
                        res.set_body(ResBody::Once(data.into()));
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "compression failed");
                        res.set_body(ResBody::Chunks(chunks));
                        return;
                    }
                }
            }
            ResBody::Stream(stream) => {
                let stream = stream.map(|item| item.map_err(|_| ErrorKind::Other));
                let reader = StreamReader::new(stream);
                match algo {
                    CompressionAlgo::Gzip => {
                        let stream = ReaderStream::new(GzipEncoder::new(reader));
                        if let Err(e) = res.streaming(stream) {
                            tracing::error!(error = ?e, "request streaming error");
                        }
                    }
                    CompressionAlgo::Deflate => {
                        let stream = ReaderStream::new(DeflateEncoder::new(reader));
                        if let Err(e) = res.streaming(stream) {
                            tracing::error!(error = ?e, "request streaming error");
                        }
                    }
                    CompressionAlgo::Brotli => {
                        let stream = ReaderStream::new(BrotliEncoder::new(reader));
                        if let Err(e) = res.streaming(stream) {
                            tracing::error!(error = ?e, "request streaming error");
                        }
                    }
                }
            }
            _ => {}
        }
        res.headers_mut().remove(CONTENT_LENGTH);
        res.headers_mut().append(CONTENT_ENCODING, algo.into());
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::http::header::{ACCEPT_ENCODING, CONTENT_ENCODING};
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[handler]
    async fn hello() -> &'static str {
        "hello"
    }

    #[tokio::test]
    async fn test_gzip() {
        let comp_handler = Compression::new().with_min_length(1);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));

        let mut res = TestClient::get("http://127.0.0.1:7979/hello")
            .add_header(ACCEPT_ENCODING, "gzip", true)
            .send(router)
            .await;
        assert_eq!(res.headers().get(CONTENT_ENCODING).unwrap(), "gzip");
        let content = res.take_string().await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_brotli() {
        let comp_handler = Compression::new().with_min_length(1);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));

        let mut res = TestClient::get("http://127.0.0.1:7979/hello")
            .add_header(ACCEPT_ENCODING, "br", true)
            .send(router)
            .await;
        assert_eq!(res.headers().get(CONTENT_ENCODING).unwrap(), "br");
        let content = res.take_string().await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_deflate() {
        let comp_handler = Compression::new().with_min_length(1);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));

        let mut res = TestClient::get("http://127.0.0.1:7979/hello")
            .add_header(ACCEPT_ENCODING, "deflate", true)
            .send(router)
            .await;
        assert_eq!(res.headers().get(CONTENT_ENCODING).unwrap(), "deflate");
        let content = res.take_string().await.unwrap();
        assert_eq!(content, "hello");
    }
}
