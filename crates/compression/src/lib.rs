#![cfg_attr(docsrs, feature(doc_cfg))]

//! Compression middleware for the Salvo web framework.
//!
//! This middleware automatically compresses HTTP responses using various algorithms,
//! reducing bandwidth usage and improving load times for clients.
//!
//! # Supported Algorithms
//!
//! | Algorithm | Feature | Content-Encoding |
//! |-----------|---------|------------------|
//! | Gzip | `gzip` | `gzip` |
//! | Brotli | `brotli` | `br` |
//! | Deflate | `deflate` | `deflate` |
//! | Zstd | `zstd` | `zstd` |
//!
//! # Example
//!
//! ```ignore
//! use salvo_compression::{Compression, CompressionLevel};
//! use salvo_core::prelude::*;
//!
//! let compression = Compression::new()
//!     .enable_gzip(CompressionLevel::Default)
//!     .min_length(1024);  // Only compress responses > 1KB
//!
//! let router = Router::new()
//!     .hoop(compression)
//!     .get(my_handler);
//! ```
//!
//! # Algorithm Negotiation
//!
//! The middleware negotiates the compression algorithm based on the client's
//! `Accept-Encoding` header. By default, it respects the client's preference order.
//! Use `force_priority(true)` to use the server's configured priority instead.
//!
//! # Compression Levels
//!
//! - [`CompressionLevel::Fastest`]: Fastest compression, larger output
//! - [`CompressionLevel::Default`]: Balanced compression (recommended)
//! - [`CompressionLevel::Minsize`]: Best compression, slower
//! - `CompressionLevel::Precise(u32)`: Fine-grained control
//!
//! # Default Content Types
//!
//! By default, the middleware compresses:
//! - `text/*` (HTML, CSS, plain text, etc.)
//! - `application/javascript`
//! - `application/json`
//! - `application/xml`, `application/rss+xml`
//! - `application/wasm`
//! - `image/svg+xml`
//!
//! Use `.content_types()` to customize which MIME types are compressed.
//!
//! # Minimum Length
//!
//! Small responses may not benefit from compression. Use `.min_length(bytes)`
//! to skip compression for responses smaller than the specified size.
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
            "br" => Ok(Self::Brotli),
            #[cfg(feature = "brotli")]
            "brotli" => Ok(Self::Brotli),

            #[cfg(feature = "deflate")]
            "deflate" => Ok(Self::Deflate),

            #[cfg(feature = "gzip")]
            "gzip" => Ok(Self::Gzip),

            #[cfg(feature = "zstd")]
            "zstd" => Ok(Self::Zstd),
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
            Self::Brotli => write!(f, "br"),
            #[cfg(feature = "deflate")]
            Self::Deflate => write!(f, "deflate"),
            #[cfg(feature = "gzip")]
            Self::Gzip => write!(f, "gzip"),
            #[cfg(feature = "zstd")]
            Self::Zstd => write!(f, "zstd"),
            _ => unreachable!(),
        }
    }
}

impl From<CompressionAlgo> for HeaderValue {
    #[inline]
    fn from(algo: CompressionAlgo) -> Self {
        match algo {
            #[cfg(feature = "brotli")]
            CompressionAlgo::Brotli => Self::from_static("br"),
            #[cfg(feature = "deflate")]
            CompressionAlgo::Deflate => Self::from_static("deflate"),
            #[cfg(feature = "gzip")]
            CompressionAlgo::Gzip => Self::from_static("gzip"),
            #[cfg(feature = "zstd")]
            CompressionAlgo::Zstd => Self::from_static("zstd"),
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
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }

    /// Remove all compression algorithms.
    #[inline]
    #[must_use]
    pub fn disable_all(mut self) -> Self {
        self.algos.clear();
        self
    }

    /// Sets `Compression` with algos.
    #[cfg(feature = "gzip")]
    #[cfg_attr(docsrs, doc(cfg(feature = "gzip")))]
    #[inline]
    #[must_use]
    pub fn enable_gzip(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Gzip, level);
        self
    }
    /// Disable gzip compression.
    #[cfg(feature = "gzip")]
    #[cfg_attr(docsrs, doc(cfg(feature = "gzip")))]
    #[inline]
    #[must_use]
    pub fn disable_gzip(mut self) -> Self {
        self.algos.shift_remove(&CompressionAlgo::Gzip);
        self
    }
    /// Enable zstd compression.
    #[cfg(feature = "zstd")]
    #[cfg_attr(docsrs, doc(cfg(feature = "zstd")))]
    #[inline]
    #[must_use]
    pub fn enable_zstd(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Zstd, level);
        self
    }
    /// Disable zstd compression.
    #[cfg(feature = "zstd")]
    #[cfg_attr(docsrs, doc(cfg(feature = "zstd")))]
    #[inline]
    #[must_use]
    pub fn disable_zstd(mut self) -> Self {
        self.algos.shift_remove(&CompressionAlgo::Zstd);
        self
    }
    /// Enable brotli compression.
    #[cfg(feature = "brotli")]
    #[cfg_attr(docsrs, doc(cfg(feature = "brotli")))]
    #[inline]
    #[must_use]
    pub fn enable_brotli(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Brotli, level);
        self
    }
    /// Disable brotli compression.
    #[cfg(feature = "brotli")]
    #[cfg_attr(docsrs, doc(cfg(feature = "brotli")))]
    #[inline]
    #[must_use]
    pub fn disable_brotli(mut self) -> Self {
        self.algos.shift_remove(&CompressionAlgo::Brotli);
        self
    }

    /// Enable deflate compression.
    #[cfg(feature = "deflate")]
    #[cfg_attr(docsrs, doc(cfg(feature = "deflate")))]
    #[inline]
    #[must_use]
    pub fn enable_deflate(mut self, level: CompressionLevel) -> Self {
        self.algos.insert(CompressionAlgo::Deflate, level);
        self
    }

    /// Disable deflate compression.
    #[cfg(feature = "deflate")]
    #[cfg_attr(docsrs, doc(cfg(feature = "deflate")))]
    #[inline]
    #[must_use]
    pub fn disable_deflate(mut self) -> Self {
        self.algos.shift_remove(&CompressionAlgo::Deflate);
        self
    }

    /// Sets minimum compression size, if body is less than this value, no compression
    /// default is 1kb
    #[inline]
    #[must_use]
    pub fn min_length(mut self, size: usize) -> Self {
        self.min_length = size;
        self
    }
    /// Sets `Compression` with force_priority.
    #[inline]
    #[must_use]
    pub fn force_priority(mut self, force_priority: bool) -> Self {
        self.force_priority = force_priority;
        self
    }

    /// Sets `Compression` with content types list.
    #[inline]
    #[must_use]
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

        if let Some(StatusCode::SWITCHING_PROTOCOLS | StatusCode::NO_CONTENT) = res.status_code {
            return;
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
                if let Some((algo, level)) = self.negotiate(req, res) {
                    res.stream(EncodeStream::new(algo, level, Some(bytes)));
                    res.headers_mut().append(CONTENT_ENCODING, algo.into());
                } else {
                    res.body(ResBody::Once(bytes));
                    return;
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
                if let Some((algo, level)) = self.negotiate(req, res) {
                    res.stream(EncodeStream::new(algo, level, chunks));
                    res.headers_mut().append(CONTENT_ENCODING, algo.into());
                } else {
                    res.body(ResBody::Chunks(chunks));
                    return;
                }
            }
            ResBody::Hyper(body) => {
                if let Some((algo, level)) = self.negotiate(req, res) {
                    res.stream(EncodeStream::new(algo, level, body));
                    res.headers_mut().append(CONTENT_ENCODING, algo.into());
                } else {
                    res.body(ResBody::Hyper(body));
                    return;
                }
            }
            ResBody::Stream(body) => {
                let body = body.into_inner();
                if let Some((algo, level)) = self.negotiate(req, res) {
                    res.stream(EncodeStream::new(algo, level, body));
                    res.headers_mut().append(CONTENT_ENCODING, algo.into());
                } else {
                    res.body(ResBody::stream(body));
                    return;
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

    #[tokio::test]
    async fn test_zstd() {
        let comp_handler = Compression::new().min_length(1);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));

        let mut res = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header(ACCEPT_ENCODING, "zstd", true)
            .send(router)
            .await;
        assert_eq!(res.headers().get(CONTENT_ENCODING).unwrap(), "zstd");
        let content = res.take_string().await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_min_length_not_compress() {
        let comp_handler = Compression::new().min_length(10);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));

        let res = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header(ACCEPT_ENCODING, "gzip", true)
            .send(router)
            .await;
        assert!(res.headers().get(CONTENT_ENCODING).is_none());
    }

    #[tokio::test]
    async fn test_min_length_should_compress() {
        let comp_handler = Compression::new().min_length(1);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));

        let res = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header(ACCEPT_ENCODING, "gzip", true)
            .send(router)
            .await;
        assert!(res.headers().get(CONTENT_ENCODING).is_some());
    }

    #[handler]
    async fn hello_html(res: &mut Response) {
        res.render(Text::Html("<html><body>hello</body></html>"));
    }
    #[tokio::test]
    async fn test_content_types_should_compress() {
        let comp_handler = Compression::new()
            .min_length(1)
            .content_types(&[mime::TEXT_HTML]);
        let router =
            Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello_html));

        let res = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header(ACCEPT_ENCODING, "gzip", true)
            .send(router)
            .await;
        assert!(res.headers().get(CONTENT_ENCODING).is_some());
    }

    #[tokio::test]
    async fn test_content_types_not_compress() {
        let comp_handler = Compression::new()
            .min_length(1)
            .content_types(&[mime::APPLICATION_JSON]);
        let router =
            Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello_html));

        let res = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header(ACCEPT_ENCODING, "gzip", true)
            .send(router)
            .await;
        assert!(res.headers().get(CONTENT_ENCODING).is_none());
    }

    #[tokio::test]
    async fn test_force_priority() {
        let comp_handler = Compression::new()
            .disable_all()
            .enable_brotli(CompressionLevel::Default)
            .enable_gzip(CompressionLevel::Default)
            .min_length(1)
            .force_priority(true);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));

        let mut res = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header(ACCEPT_ENCODING, "gzip, br", true)
            .send(router)
            .await;
        assert_eq!(res.headers().get(CONTENT_ENCODING).unwrap(), "br");
        let content = res.take_string().await.unwrap();
        assert_eq!(content, "hello");
    }

    // Tests for CompressionLevel
    #[test]
    fn test_compression_level_default() {
        let level: CompressionLevel = Default::default();
        assert_eq!(level, CompressionLevel::Default);
    }

    #[test]
    fn test_compression_level_fastest() {
        let level = CompressionLevel::Fastest;
        assert_eq!(level, CompressionLevel::Fastest);
    }

    #[test]
    fn test_compression_level_minsize() {
        let level = CompressionLevel::Minsize;
        assert_eq!(level, CompressionLevel::Minsize);
    }

    #[test]
    fn test_compression_level_precise() {
        let level = CompressionLevel::Precise(5);
        assert_eq!(level, CompressionLevel::Precise(5));
    }

    #[test]
    fn test_compression_level_clone() {
        let level = CompressionLevel::Fastest;
        let cloned = level;
        assert_eq!(level, cloned);
    }

    #[test]
    fn test_compression_level_copy() {
        let level = CompressionLevel::Default;
        let copied = level;
        assert_eq!(level, copied);
    }

    #[test]
    fn test_compression_level_debug() {
        let level = CompressionLevel::Fastest;
        let debug_str = format!("{:?}", level);
        assert!(debug_str.contains("Fastest"));
    }

    // Tests for CompressionAlgo
    #[cfg(feature = "gzip")]
    #[test]
    fn test_compression_algo_gzip_from_str() {
        let algo: CompressionAlgo = "gzip".parse().unwrap();
        assert_eq!(algo, CompressionAlgo::Gzip);
    }

    #[cfg(feature = "brotli")]
    #[test]
    fn test_compression_algo_brotli_from_str() {
        let algo: CompressionAlgo = "br".parse().unwrap();
        assert_eq!(algo, CompressionAlgo::Brotli);

        let algo: CompressionAlgo = "brotli".parse().unwrap();
        assert_eq!(algo, CompressionAlgo::Brotli);
    }

    #[cfg(feature = "deflate")]
    #[test]
    fn test_compression_algo_deflate_from_str() {
        let algo: CompressionAlgo = "deflate".parse().unwrap();
        assert_eq!(algo, CompressionAlgo::Deflate);
    }

    #[cfg(feature = "zstd")]
    #[test]
    fn test_compression_algo_zstd_from_str() {
        let algo: CompressionAlgo = "zstd".parse().unwrap();
        assert_eq!(algo, CompressionAlgo::Zstd);
    }

    #[test]
    fn test_compression_algo_unknown_from_str() {
        let result: Result<CompressionAlgo, _> = "unknown".parse();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("unknown compression algorithm")
        );
    }

    #[cfg(feature = "gzip")]
    #[test]
    fn test_compression_algo_gzip_display() {
        let algo = CompressionAlgo::Gzip;
        assert_eq!(format!("{}", algo), "gzip");
    }

    #[cfg(feature = "brotli")]
    #[test]
    fn test_compression_algo_brotli_display() {
        let algo = CompressionAlgo::Brotli;
        assert_eq!(format!("{}", algo), "br");
    }

    #[cfg(feature = "deflate")]
    #[test]
    fn test_compression_algo_deflate_display() {
        let algo = CompressionAlgo::Deflate;
        assert_eq!(format!("{}", algo), "deflate");
    }

    #[cfg(feature = "zstd")]
    #[test]
    fn test_compression_algo_zstd_display() {
        let algo = CompressionAlgo::Zstd;
        assert_eq!(format!("{}", algo), "zstd");
    }

    #[cfg(feature = "gzip")]
    #[test]
    fn test_compression_algo_into_header_value() {
        let algo = CompressionAlgo::Gzip;
        let header: HeaderValue = algo.into();
        assert_eq!(header, "gzip");
    }

    #[test]
    fn test_compression_algo_debug() {
        #[cfg(feature = "gzip")]
        {
            let algo = CompressionAlgo::Gzip;
            let debug_str = format!("{:?}", algo);
            assert!(debug_str.contains("Gzip"));
        }
    }

    #[test]
    fn test_compression_algo_clone() {
        #[cfg(feature = "gzip")]
        {
            let algo = CompressionAlgo::Gzip;
            let cloned = algo;
            assert_eq!(algo, cloned);
        }
    }

    #[test]
    fn test_compression_algo_hash() {
        use std::collections::HashSet;
        #[cfg(feature = "gzip")]
        {
            let mut set = HashSet::new();
            set.insert(CompressionAlgo::Gzip);
            assert!(set.contains(&CompressionAlgo::Gzip));
        }
    }

    // Tests for Compression struct
    #[test]
    fn test_compression_new() {
        let comp = Compression::new();
        assert!(!comp.algos.is_empty());
        assert!(!comp.content_types.is_empty());
        assert_eq!(comp.min_length, 0);
        assert!(!comp.force_priority);
    }

    #[test]
    fn test_compression_default() {
        let comp = Compression::default();
        assert!(!comp.algos.is_empty());
    }

    #[test]
    fn test_compression_disable_all() {
        let comp = Compression::new().disable_all();
        assert!(comp.algos.is_empty());
    }

    #[cfg(feature = "gzip")]
    #[test]
    fn test_compression_enable_gzip() {
        let comp = Compression::new()
            .disable_all()
            .enable_gzip(CompressionLevel::Fastest);
        assert!(comp.algos.contains_key(&CompressionAlgo::Gzip));
        assert_eq!(
            comp.algos.get(&CompressionAlgo::Gzip),
            Some(&CompressionLevel::Fastest)
        );
    }

    #[cfg(feature = "gzip")]
    #[test]
    fn test_compression_disable_gzip() {
        let comp = Compression::new().disable_gzip();
        assert!(!comp.algos.contains_key(&CompressionAlgo::Gzip));
    }

    #[cfg(feature = "brotli")]
    #[test]
    fn test_compression_enable_brotli() {
        let comp = Compression::new()
            .disable_all()
            .enable_brotli(CompressionLevel::Minsize);
        assert!(comp.algos.contains_key(&CompressionAlgo::Brotli));
    }

    #[cfg(feature = "brotli")]
    #[test]
    fn test_compression_disable_brotli() {
        let comp = Compression::new().disable_brotli();
        assert!(!comp.algos.contains_key(&CompressionAlgo::Brotli));
    }

    #[cfg(feature = "zstd")]
    #[test]
    fn test_compression_enable_zstd() {
        let comp = Compression::new()
            .disable_all()
            .enable_zstd(CompressionLevel::Default);
        assert!(comp.algos.contains_key(&CompressionAlgo::Zstd));
    }

    #[cfg(feature = "zstd")]
    #[test]
    fn test_compression_disable_zstd() {
        let comp = Compression::new().disable_zstd();
        assert!(!comp.algos.contains_key(&CompressionAlgo::Zstd));
    }

    #[cfg(feature = "deflate")]
    #[test]
    fn test_compression_enable_deflate() {
        let comp = Compression::new()
            .disable_all()
            .enable_deflate(CompressionLevel::Default);
        assert!(comp.algos.contains_key(&CompressionAlgo::Deflate));
    }

    #[cfg(feature = "deflate")]
    #[test]
    fn test_compression_disable_deflate() {
        let comp = Compression::new().disable_deflate();
        assert!(!comp.algos.contains_key(&CompressionAlgo::Deflate));
    }

    #[test]
    fn test_compression_min_length() {
        let comp = Compression::new().min_length(1024);
        assert_eq!(comp.min_length, 1024);
    }

    #[test]
    fn test_compression_force_priority() {
        let comp = Compression::new().force_priority(true);
        assert!(comp.force_priority);
    }

    #[test]
    fn test_compression_content_types() {
        let comp = Compression::new().content_types(&[mime::TEXT_PLAIN, mime::TEXT_HTML]);
        assert_eq!(comp.content_types.len(), 2);
        assert!(comp.content_types.contains(&mime::TEXT_PLAIN));
        assert!(comp.content_types.contains(&mime::TEXT_HTML));
    }

    #[test]
    fn test_compression_debug() {
        let comp = Compression::new();
        let debug_str = format!("{:?}", comp);
        assert!(debug_str.contains("Compression"));
        assert!(debug_str.contains("algos"));
        assert!(debug_str.contains("content_types"));
    }

    #[test]
    fn test_compression_clone() {
        let comp = Compression::new().min_length(100);
        let cloned = comp.clone();
        assert_eq!(comp.min_length, cloned.min_length);
        assert_eq!(comp.algos.len(), cloned.algos.len());
    }

    // Tests for no compression scenarios
    #[tokio::test]
    async fn test_no_accept_encoding_header() {
        let comp_handler = Compression::new().min_length(1);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));

        let res = TestClient::get("http://127.0.0.1:5801/hello")
            .send(router)
            .await;
        assert!(res.headers().get(CONTENT_ENCODING).is_none());
    }

    #[tokio::test]
    async fn test_unsupported_encoding() {
        let comp_handler = Compression::new().min_length(1);
        let router = Router::with_hoop(comp_handler).push(Router::with_path("hello").get(hello));

        let res = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header(ACCEPT_ENCODING, "unknown", true)
            .send(router)
            .await;
        assert!(res.headers().get(CONTENT_ENCODING).is_none());
    }

    #[tokio::test]
    async fn test_empty_response() {
        #[handler]
        async fn empty() {}

        let comp_handler = Compression::new();
        let router = Router::with_hoop(comp_handler).push(Router::with_path("empty").get(empty));

        let res = TestClient::get("http://127.0.0.1:5801/empty")
            .add_header(ACCEPT_ENCODING, "gzip", true)
            .send(router)
            .await;
        assert!(res.headers().get(CONTENT_ENCODING).is_none());
    }

    #[tokio::test]
    async fn test_chained_configuration() {
        #[cfg(all(feature = "gzip", feature = "brotli"))]
        {
            let comp_handler = Compression::new()
                .disable_all()
                .enable_gzip(CompressionLevel::Fastest)
                .enable_brotli(CompressionLevel::Default)
                .min_length(1)
                .force_priority(false)
                .content_types(&[mime::TEXT_PLAIN]);

            assert_eq!(comp_handler.algos.len(), 2);
            assert_eq!(comp_handler.min_length, 1);
            assert!(!comp_handler.force_priority);
            assert_eq!(comp_handler.content_types.len(), 1);
        }
    }
}
