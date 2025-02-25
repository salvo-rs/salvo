#![cfg_attr(docsrs, feature(doc_cfg))]

//! Compression middleware for the Salvo web framework.
//!
//! Read more: <https://salvo.rs>

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use indexmap::IndexMap;

use salvo_core::http::body::ResBody;
use salvo_core::http::header::{
    ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, HeaderValue,
};
use salvo_core::http::{self, Mime, StatusCode, mime};
use salvo_core::{Depot, FlowCtrl, Handler, Request, Response, async_trait};

mod encoder;
mod stream;
use encoder::Encoder;
use stream::EncodeStream;

/// Level of compression data should be compressed with.
#[non_exhaustive]
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub enum CompressionLevel {
    /// Fastest quality of compression, usually produces a bigger size.
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
    /// Compress use Brotli algo.
    #[cfg(feature = "brotli")]
    #[cfg_attr(docsrs, doc(cfg(feature = "brotli")))]
    Brotli,

    /// Compress use Deflate algo.
    #[cfg(feature = "deflate")]
    #[cfg_attr(docsrs, doc(cfg(feature = "deflate")))]
    Deflate,

    /// Compress use Gzip algo.
    #[cfg(feature = "gzip")]
    #[cfg_attr(docsrs, doc(cfg(feature = "gzip")))]
    Gzip,

    /// Compress use Zstd algo.
    #[cfg(feature = "zstd")]
    #[cfg_attr(docsrs, doc(cfg(feature = "zstd")))]
    Zstd,
}

impl FromStr for CompressionAlgo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            #[cfg(feature = "brotli")]
            "br" => Ok(CompressionAlgo::Brotli),
            #[cfg(feature = "brotli")]
            "brotli" => Ok(CompressionAlgo::Brotli),

            #[cfg(feature = "deflate")]
            "deflate" => Ok(CompressionAlgo::Deflate),

            #[cfg(feature = "gzip")]
            "gzip" => Ok(CompressionAlgo::Gzip),

            #[cfg(feature = "zstd")]
            "zstd" => Ok(CompressionAlgo::Zstd),
            _ => Err(format!("unknown compression algorithm: {s}")),
        }
    }
}

impl Display for CompressionAlgo {
    #[allow(unreachable_patterns)]
    #[allow(unused_variables)]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "brotli")]
            CompressionAlgo::Brotli => write!(f, "br"),
            #[cfg(feature = "deflate")]
            CompressionAlgo::Deflate => write!(f, "deflate"),
            #[cfg(feature = "gzip")]
            CompressionAlgo::Gzip => write!(f, "gzip"),
            #[cfg(feature = "zstd")]
            CompressionAlgo::Zstd => write!(f, "zstd"),
            _ => unreachable!(),
        }
    }
}

impl From<CompressionAlgo> for HeaderValue {
    #[inline]
    fn from(algo: CompressionAlgo) -> Self {
        match algo {
            #[cfg(feature = "brotli")]
            CompressionAlgo::Brotli => HeaderValue::from_static("br"),
            #[cfg(feature = "deflate")]
            CompressionAlgo::Deflate => HeaderValue::from_static("deflate"),
            #[cfg(feature = "gzip")]
            CompressionAlgo::Gzip => HeaderValue::from_static("gzip"),
            #[cfg(feature = "zstd")]
            CompressionAlgo::Zstd => HeaderValue::from_static("zstd"),
        }
    }
}

/// Compression
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Compression {
    /// Compression algorithms to use.
    pub algos: IndexMap<CompressionAlgo, CompressionLevel>,
    /// Content types to compress.
    pub content_types: Vec<Mime>,
    /// Sets minimum compression size, if body is less than this value, no compression.
    pub min_length: usize,
    /// Ignore request algorithms order in `Accept-Encoding` header and always server's config.
    pub force_priority: bool,
}

impl Default for Compression {
    fn default() -> Self {
        #[allow(unused_mut)]
        let mut algos = IndexMap::new();
        #[cfg(feature = "zstd")]
        algos.insert(CompressionAlgo::Zstd, CompressionLevel::Default);
        #[cfg(feature = "gzip")]
        algos.insert(CompressionAlgo::Gzip, CompressionLevel::Default);
        #[cfg(feature = "deflate")]
        algos.insert(CompressionAlgo::Deflate, CompressionLevel::Default);
        #[cfg(feature = "brotli")]
        algos.insert(CompressionAlgo::Brotli, CompressionLevel::Default);
        Self {
            algos,
            content_types: vec![
                mime::TEXT_STAR,
                mime::APPLICATION_JAVASCRIPT,
                mime::APPLICATION_JSON,
                mime::IMAGE_SVG,
                "application/wasm".parse().expect("invalid mime type"),
                "application/xml".parse().expect("invalid mime type"),
                "application/rss+xml".parse().expect("invalid mime type"),
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
    #[cfg(feature = "gzip")]
    #[cfg_attr(docsrs, doc(cfg(feature = "gzip")))]
    #[inline]
    pub fn enable_gzip(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Gzip, level);
        self
    }
    /// Disable gzip compression.
    #[cfg(feature = "gzip")]
    #[cfg_attr(docsrs, doc(cfg(feature = "gzip")))]
    #[inline]
    pub fn disable_gzip(mut self) -> Self {
        self.algos.shift_remove(&CompressionAlgo::Gzip);
        self
    }
    /// Enable zstd compression.
    #[cfg(feature = "zstd")]
    #[cfg_attr(docsrs, doc(cfg(feature = "zstd")))]
    #[inline]
    pub fn enable_zstd(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Zstd, level);
        self
    }
    /// Disable zstd compression.
    #[cfg(feature = "zstd")]
    #[cfg_attr(docsrs, doc(cfg(feature = "zstd")))]
    #[inline]
    pub fn disable_zstd(mut self) -> Self {
        self.algos.shift_remove(&CompressionAlgo::Zstd);
        self
    }
    /// Enable brotli compression.
    #[cfg(feature = "brotli")]
    #[cfg_attr(docsrs, doc(cfg(feature = "brotli")))]
    #[inline]
    pub fn enable_brotli(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Brotli, level);
        self
    }
    /// Disable brotli compression.
    #[cfg(feature = "brotli")]
    #[cfg_attr(docsrs, doc(cfg(feature = "brotli")))]
    #[inline]
    pub fn disable_brotli(mut self) -> Self {
        self.algos.shift_remove(&CompressionAlgo::Brotli);
        self
    }

    /// Enable deflate compression.
    #[cfg(feature = "deflate")]
    #[cfg_attr(docsrs, doc(cfg(feature = "deflate")))]
    #[inline]
    pub fn enable_deflate(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Deflate, level);
        self
    }

    /// Disable deflate compression.
    #[cfg(feature = "deflate")]
    #[cfg_attr(docsrs, doc(cfg(feature = "deflate")))]
    #[inline]
    pub fn disable_deflate(mut self) -> Self {
        self.algos.shift_remove(&CompressionAlgo::Deflate);
        self
    }

    /// Sets minimum compression size, if body is less than this value, no compression
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

    fn negotiate(
        &self,
        req: &Request,
        res: &Response,
    ) -> Option<(CompressionAlgo, CompressionLevel)> {
        if req.headers().contains_key(&CONTENT_ENCODING) {
            return None;
        }

        if !self.content_types.is_empty() {
            let content_type = res
                .headers()
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
        let header = req
            .headers()
            .get(ACCEPT_ENCODING)
            .and_then(|v| v.to_str().ok())?;

        let accept_algos = http::parse_accept_encoding(header)
            .into_iter()
            .filter_map(|(algo, level)| {
                if let Ok(algo) = algo.parse::<CompressionAlgo>() {
                    Some((algo, level))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
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

        if let Some(code) = res.status_code {
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
                    res.body(ResBody::Once(bytes));
                    return;
                }
                match self.negotiate(req, res) {
                    Some((algo, level)) => {
                        res.stream(EncodeStream::new(algo, level, Some(bytes)));
                        res.headers_mut().append(CONTENT_ENCODING, algo.into());
                    }
                    None => {
                        res.body(ResBody::Once(bytes));
                        return;
                    }
                }
            }
            ResBody::Chunks(chunks) => {
                if self.min_length > 0 {
                    let len: usize = chunks.iter().map(|c| c.len()).sum();
                    if len < self.min_length {
                        res.body(ResBody::Chunks(chunks));
                        return;
                    }
                }
                match self.negotiate(req, res) {
                    Some((algo, level)) => {
                        res.stream(EncodeStream::new(algo, level, chunks));
                        res.headers_mut().append(CONTENT_ENCODING, algo.into());
                    }
                    None => {
                        res.body(ResBody::Chunks(chunks));
                        return;
                    }
                }
            }
            ResBody::Hyper(body) => match self.negotiate(req, res) {
                Some((algo, level)) => {
                    res.stream(EncodeStream::new(algo, level, body));
                    res.headers_mut().append(CONTENT_ENCODING, algo.into());
                }
                None => {
                    res.body(ResBody::Hyper(body));
                    return;
                }
            },
            ResBody::Stream(body) => {
                let body = body.into_inner();
                match self.negotiate(req, res) {
                    Some((algo, level)) => {
                        res.stream(EncodeStream::new(algo, level, body));
                        res.headers_mut().append(CONTENT_ENCODING, algo.into());
                    }
                    None => {
                        res.body(ResBody::stream(body));
                        return;
                    }
                }
            }
            body => {
                res.body(body);
                return;
            }
        }
        res.headers_mut().remove(CONTENT_LENGTH);
    }
}

#[cfg(test)]
mod tests {
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
