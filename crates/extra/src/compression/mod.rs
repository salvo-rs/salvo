//! Compress the body of a response.
use std::collections::HashMap;
use std::io::{self, Error as IoError, ErrorKind, Write};
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use flate2::write::{GzEncoder, ZlibEncoder};
use futures_util::stream::{BoxStream, Stream};
use hyper::HeaderMap;
use indexmap::IndexMap;
use jsonwebtoken::Header;
use tokio_stream::{self};
use tokio_util::io::{ReaderStream, StreamReader};
use zstd::stream::raw::Operation;
use zstd::stream::write::Encoder as ZstdEncoder;

use salvo_core::http::body::{Body, HyperBody, ResBody};
use salvo_core::http::header::{HeaderValue, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE};
use salvo_core::http::StatusCode;
use salvo_core::{async_trait, BoxedError, Depot, FlowCtrl, Handler, Request, Response};

mod encoder;
mod stream;
use encoder::Encoder;
use stream::EncodeStream;

/// Level of compression data should be compressed with.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompressLevel {
    /// Fastest quality of compression, usually produces bigger size.
    Fastest,
    /// Best quality of compression, usually produces the smallest size.
    Minsize,
    /// Default quality of compression defined by the selected compression algorithm.
    Default,
    /// Precise quality based on the underlying compression algorithms'
    /// qualities. The interpretation of this depends on the algorithm chosen
    /// and the specific implementation backing it.
    /// Qualities are implicitly clamped to the algorithm's maximum.
    Precise(u32),
}

impl Default for CompressLevel {
    fn default() -> Self {
        CompressLevel::Default
    }
}

/// CompressAlgo
#[derive(Eq, PartialEq, Clone, Copy, Debug, Hash)]
#[non_exhaustive]
pub enum CompressAlgo {
    /// Gzip
    Gzip,
    /// Deflate
    Deflate,
    /// Brotli
    Brotli,
    /// Zstd
    Zstd,
}
impl CompressAlgo {}

impl FromStr for CompressAlgo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "br" => Ok(CompressAlgo::Brotli),
            "gzip" => Ok(CompressAlgo::Gzip),
            "deflate" => Ok(CompressAlgo::Deflate),
            "zstd" => Ok(CompressAlgo::Zstd),
            _ => Err(format!("unknown compression algorithm: {s}")),
        }
    }
}

impl From<CompressAlgo> for HeaderValue {
    #[inline]
    fn from(algo: CompressAlgo) -> Self {
        match algo {
            CompressAlgo::Gzip => HeaderValue::from_static("gzip"),
            CompressAlgo::Deflate => HeaderValue::from_static("deflate"),
            CompressAlgo::Brotli => HeaderValue::from_static("br"),
            CompressAlgo::Zstd => HeaderValue::from_static("zstd"),
        }
    }
}

/// Compression
#[derive(Clone, Debug)]
pub struct Compression {
    pub algos: IndexMap<CompressAlgo, CompressLevel>,
    pub content_types: Vec<String>,
    pub min_length: usize,
    pub force_priority: bool,
}

impl Default for Compression {
    #[inline]
    fn default() -> Self {
        let mut algos = IndexMap::new();
        algos.insert(CompressAlgo::Zstd, CompressLevel::Default);
        algos.insert(CompressAlgo::Gzip, CompressLevel::Default);
        algos.insert(CompressAlgo::Deflate, CompressLevel::Default);
        algos.insert(CompressAlgo::Brotli, CompressLevel::Default);
        Self {
            algos,
            content_types: vec![
                "text/".into(),
                "application/javascript".into(),
                "application/json".into(),
                "application/xml".into(),
                "application/rss+xml".into(),
                "application/wasm".into(),
                "image/svg+xml".into(),
            ],
            min_length: 0,
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
    pub fn enable_gzip(mut self, level: CompressLevel) -> Self {
        self.algos.insert(CompressAlgo::Gzip, level);
        self
    }
    #[inline]
    pub fn disable_gzip(mut self) -> Self {
        self.algos.remove(&CompressAlgo::Gzip);
        self
    }
    #[inline]
    pub fn enable_zstd(mut self, level: CompressLevel) -> Self {
        self.algos.insert(CompressAlgo::Zstd, level);
        self
    }
    #[inline]
    pub fn disable_zstd(mut self) -> Self {
        self.algos.remove(&CompressAlgo::Zstd);
        self
    }
    #[inline]
    pub fn enable_brotli(mut self, level: CompressLevel) -> Self {
        self.algos.insert(CompressAlgo::Brotli, level);
        self
    }
    #[inline]
    pub fn disable_brotli(mut self) -> Self {
        self.algos.remove(&CompressAlgo::Brotli);
        self
    }
    #[inline]
    pub fn enable_deflate(mut self, level: CompressLevel) -> Self {
        self.algos.insert(CompressAlgo::Deflate, level);
        self
    }
    #[inline]
    pub fn disable_deflate(mut self) -> Self {
        self.algos.remove(&CompressAlgo::Deflate);
        self
    }

    /// Sets minimum compression size, if body less than this value, no compression
    /// default is 1kb
    #[inline]
    pub fn min_length(mut self, size: usize) -> Self {
        self.min_length = size;
        self
    }
    /// Sets `Compression` with force_priority.
    #[inline]
    pub fn force_priority(mut self, force_priority: bool) -> Self {
        self.force_priority = force_priority;
        self
    }

    /// Sets `Compression` with content types list.
    #[inline]
    pub fn content_types(mut self, content_types: &[String]) -> Self {
        self.content_types = content_types.to_vec();
        self
    }

    fn negotiate(&self, headers: &HeaderMap) -> Option<(CompressAlgo, CompressLevel)> {
        if headers.contains_key(&CONTENT_ENCODING) {
            return None;
        }

        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if content_type.is_empty() || !self.content_types.iter().any(|c| content_type.starts_with(&**c)) {
            return None;
        }

        let header = headers.get(ACCEPT_ENCODING).and_then(|v| v.to_str().ok())?;

        let accept_algos = parse_accept_encoding(header);
        if self.force_priority {
            let accept_algos = accept_algos.into_iter().map(|(algo, _)| algo).collect::<Vec<_>>();
            self.algos
                .iter()
                .find(|(algo, level)| accept_algos.contains(algo))
                .map(|(algo, level)| (*algo, *level))
        } else {
            accept_algos.into_iter().find_map(|(algo, _)| {
                if let Some(level) = self.algos.get(&algo) {
                    Some((algo, *level))
                } else {
                    None
                }
            })
        }
    }
}

fn parse_accept_encoding(header: &str) -> Vec<(CompressAlgo, u8)> {
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
        .collect::<Vec<(CompressAlgo, u8)>>();

    vec.sort_by(|(_, a), (_, b)| match b.cmp(a) {
        std::cmp::Ordering::Equal => std::cmp::Ordering::Greater,
        other => other,
    });

    vec
}

#[async_trait]
impl Handler for Compression {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        ctrl.call_next(req, depot, res).await;
        if ctrl.is_ceased() || res.headers().contains_key(CONTENT_ENCODING) {
            return;
        }

        if let Some(code) = res.status_code() {
            if code == StatusCode::SWITCHING_PROTOCOLS || code == StatusCode::NO_CONTENT {
                return;
            }
        } else {
            return;
        }

        match res.take_body() {
            ResBody::None => {
                return;
            }
            ResBody::Once(bytes) => {
                if self.min_length == 0 || bytes.len() < self.min_length {
                    res.set_body(ResBody::Once(bytes));
                    return;
                }
                match self.negotiate(req.headers()) {
                    Some((algo, level)) => {
                        res.streaming(EncodeStream::new(algo, level, Some(bytes)));
                        res.headers_mut().append(CONTENT_ENCODING, algo.into());
                    }
                    None => {
                        res.set_body(ResBody::Once(bytes));
                        return;
                    }
                }
            }
            ResBody::Chunks(chunks) => {
                if self.min_length > 0 {
                    let len: usize = chunks.iter().map(|c| c.len()).sum();
                    if len < self.min_length {
                        res.set_body(ResBody::Chunks(chunks));
                        return;
                    }
                }
                match self.negotiate(req.headers()) {
                    Some((algo, level)) => {
                        res.streaming(EncodeStream::new(algo, level, chunks));
                        res.headers_mut().append(CONTENT_ENCODING, algo.into());
                    }
                    None => {
                        res.set_body(ResBody::Chunks(chunks));
                        return;
                    }
                }
            }
            ResBody::Hyper(body) => match self.negotiate(req.headers()) {
                Some((algo, level)) => {
                    res.streaming(EncodeStream::new(algo, level, body));
                    res.headers_mut().append(CONTENT_ENCODING, algo.into());
                }
                None => {
                    res.set_body(ResBody::Hyper(body));
                    return;
                }
            },
            ResBody::Stream(body) => match self.negotiate(req.headers()) {
                Some((algo, level)) => {
                    res.streaming(EncodeStream::new(algo, level, body));
                    res.headers_mut().append(CONTENT_ENCODING, algo.into());
                }
                None => {
                    res.set_body(ResBody::Stream(body));
                    return;
                }
            },
            _ => {}
        }
        res.headers_mut().remove(CONTENT_LENGTH);
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

        let mut res = TestClient::get("http://127.0.0.1:5801/hello")
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

        let mut res = TestClient::get("http://127.0.0.1:5801/hello")
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

        let mut res = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header(ACCEPT_ENCODING, "deflate", true)
            .send(router)
            .await;
        assert_eq!(res.headers().get(CONTENT_ENCODING).unwrap(), "deflate");
        let content = res.take_string().await.unwrap();
        assert_eq!(content, "hello");
    }
}
