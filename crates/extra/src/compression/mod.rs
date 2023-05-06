//! Compress the body of a response.
use std::str::FromStr;

use indexmap::IndexMap;

use salvo_core::http::body::ResBody;
use salvo_core::http::header::{
    HeaderValue, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE,
};
use salvo_core::http::{Mime, StatusCode};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Request, Response};

mod encoder;
mod stream;
use encoder::Encoder;
use stream::EncodeStream;

/// Level of compression data should be compressed with.
#[non_exhaustive]
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub enum CompressionLevel {
    /// Fastest quality of compression, usually produces bigger size.
    Fastest,
    /// Best quality of compression, usually produces the smallest size.
    Minsize,
    /// Default quality of compression defined by the selected compression algorithm.
    #[default]
    Default,
    /// Precise quality based on the underlying compression algorithms'
    /// qualities. The interpretation of this depends on the algorithm chosen
    /// and the specific implementation backing it.
    /// Qualities are implicitly clamped to the algorithm's maximum.
    Precise(u32),
}

/// CompressionAlgo
#[derive(Eq, PartialEq, Clone, Copy, Debug, Hash)]
#[non_exhaustive]
pub enum CompressionAlgo {
    /// Gzip
    Gzip,
    /// Deflate
    Deflate,
    /// Brotli
    Brotli,
    /// Zstd
    Zstd,
}
impl CompressionAlgo {}

impl FromStr for CompressionAlgo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "br" => Ok(CompressionAlgo::Brotli),
            "gzip" => Ok(CompressionAlgo::Gzip),
            "deflate" => Ok(CompressionAlgo::Deflate),
            "zstd" => Ok(CompressionAlgo::Zstd),
            _ => Err(format!("unknown compression algorithm: {s}")),
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
            CompressionAlgo::Zstd => HeaderValue::from_static("zstd"),
        }
    }
}

/// Compression
#[derive(Clone, Debug)]
pub struct Compression {
    /// Compression algorithms to use.
    pub algos: IndexMap<CompressionAlgo, CompressionLevel>,
    /// Content types to compress.
    pub content_types: Vec<Mime>,
    /// Sets minimum compression size, if body less than this value, no compression.
    pub min_length: usize,
    /// Ignore request algorithms order in `Accept-Encoding` header and always server's config.
    pub force_priority: bool,
}

impl Default for Compression {
    #[inline]
    fn default() -> Self {
        let mut algos = IndexMap::new();
        algos.insert(CompressionAlgo::Zstd, CompressionLevel::Default);
        algos.insert(CompressionAlgo::Gzip, CompressionLevel::Default);
        algos.insert(CompressionAlgo::Deflate, CompressionLevel::Default);
        algos.insert(CompressionAlgo::Brotli, CompressionLevel::Default);
        Self {
            algos,
            content_types: vec![
                "text/*".parse().unwrap(),
                "application/javascript".parse().unwrap(),
                "application/json".parse().unwrap(),
                "application/xml".parse().unwrap(),
                "application/rss+xml".parse().unwrap(),
                "application/wasm".parse().unwrap(),
                "image/svg+xml".parse().unwrap(),
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
    
    /// Remove all compression algorithms.
    #[inline]
    pub fn disable_all(mut self) -> Self {
        self.algos.clear();
        self
    }

    /// Sets `Compression` with algos.
    #[inline]
    pub fn enable_gzip(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Gzip, level);
        self
    }
    /// Disable gzip compression.
    #[inline]
    pub fn disable_gzip(mut self) -> Self {
        self.algos.remove(&CompressionAlgo::Gzip);
        self
    }
    /// Enable zstd compression.
    #[inline]
    pub fn enable_zstd(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Zstd, level);
        self
    }
    /// Disable zstd compression.
    #[inline]
    pub fn disable_zstd(mut self) -> Self {
        self.algos.remove(&CompressionAlgo::Zstd);
        self
    }
    /// Enable brotli compression.
    #[inline]
    pub fn enable_brotli(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Brotli, level);
        self
    }
    /// Disable brotli compression.
    #[inline]
    pub fn disable_brotli(mut self) -> Self {
        self.algos.remove(&CompressionAlgo::Brotli);
        self
    }

    /// Enable deflate compression.
    #[inline]
    pub fn enable_deflate(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Deflate, level);
        self
    }

    /// Disable deflate compression.
    #[inline]
    pub fn disable_deflate(mut self) -> Self {
        self.algos.remove(&CompressionAlgo::Deflate);
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
    pub fn content_types(mut self, content_types: &[Mime]) -> Self {
        self.content_types = content_types.to_vec();
        self
    }

    fn negotiate(&self, req: &Request, res: &Response) -> Option<(CompressionAlgo, CompressionLevel)> {
        if req.headers().contains_key(&CONTENT_ENCODING) {
            return None;
        }

        if !self.content_types.is_empty() {
            let content_type = res.headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();
            if content_type.is_empty() {
                return None;
            }
            if let Ok(content_type) = content_type.parse::<Mime>() {
                if !self.content_types.iter().any(|citem| {
                    citem.type_() == content_type.type_()
                        && (citem.subtype() == "*" || citem.subtype() == content_type.subtype())
                }) {
                    return None;
                }
            } else {
                return None;
            }
        }
        let header = req.headers().get(ACCEPT_ENCODING).and_then(|v| v.to_str().ok())?;

        let accept_algos = parse_accept_encoding(header);
        if self.force_priority {
            let accept_algos = accept_algos
                .into_iter()
                .map(|(algo, _)| algo)
                .collect::<Vec<_>>();
            self.algos
                .iter()
                .find(|(algo, _level)| accept_algos.contains(algo))
                .map(|(algo, level)| (*algo, *level))
        } else {
            accept_algos
                .into_iter()
                .find_map(|(algo, _)| self.algos.get(&algo).map(|level| (algo, *level)))
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

#[async_trait]
impl Handler for Compression {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        ctrl.call_next(req, depot, res).await;
        if ctrl.is_ceased() || res.headers().contains_key(CONTENT_ENCODING) {
            return;
        }

        if let Some(code) = res.status_code() {
            if code == StatusCode::SWITCHING_PROTOCOLS || code == StatusCode::NO_CONTENT {
                return;
            }
        }

        match res.take_body() {
            ResBody::None => {
                return;
            }
            ResBody::Once(bytes) => {
                if self.min_length > 0 && bytes.len() < self.min_length {
                    res.set_body(ResBody::Once(bytes));
                    return;
                }
                match self.negotiate(req, res) {
                    Some((algo, level)) => {
                        res.streaming(EncodeStream::new(algo, level, Some(bytes)))
                            .ok();
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
                match self.negotiate(req, res) {
                    Some((algo, level)) => {
                        res.streaming(EncodeStream::new(algo, level, chunks)).ok();
                        res.headers_mut().append(CONTENT_ENCODING, algo.into());
                    }
                    None => {
                        res.set_body(ResBody::Chunks(chunks));
                        return;
                    }
                }
            }
            ResBody::Hyper(body) => match self.negotiate(req, res) {
                Some((algo, level)) => {
                    res.streaming(EncodeStream::new(algo, level, body)).ok();
                    res.headers_mut().append(CONTENT_ENCODING, algo.into());
                }
                None => {
                    res.set_body(ResBody::Hyper(body));
                    return;
                }
            },
            ResBody::Stream(body) => match self.negotiate(req, res) {
                Some((algo, level)) => {
                    res.streaming(EncodeStream::new(algo, level, body)).ok();
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
        let comp_handler = Compression::new().min_length(1);
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
        let comp_handler = Compression::new().min_length(1);
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
        let comp_handler = Compression::new().min_length(1);
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
