//! Serve static directories with directory listing support

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::{self, Debug, Display, Formatter, Write};
use std::fs::Metadata;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime};

use salvo_core::fs::NamedFile;
use salvo_core::handler::Handler;
use salvo_core::http::header::{ACCEPT_ENCODING, VARY};
use salvo_core::http::{
    self, HeaderMap, HeaderValue, Request, Response, StatusCode, StatusError, mime,
};
use salvo_core::routing::{
    decode_url_path, encode_url_path, normalize_url_path, redirect_to_dir_url,
};
use salvo_core::writing::Text;
use salvo_core::{Depot, FlowCtrl, IntoVecString, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use time::macros::format_description;

use super::join_path;

/// Supported compression algorithms for serving compressed file variants
#[derive(Eq, PartialEq, Clone, Copy, Debug, Hash)]
#[non_exhaustive]
pub enum CompressionAlgo {
    /// Brotli compression
    Brotli,
    /// Deflate compression
    Deflate,
    /// Gzip compression
    Gzip,
    /// Zstandard compression
    Zstd,
}
impl FromStr for CompressionAlgo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "br" | "brotli" => Ok(Self::Brotli),
            "deflate" => Ok(Self::Deflate),
            "gzip" => Ok(Self::Gzip),
            "zstd" => Ok(Self::Zstd),
            _ => Err(format!("unknown compression algorithm: {s}")),
        }
    }
}

impl Display for CompressionAlgo {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Brotli => write!(f, "br"),
            Self::Deflate => write!(f, "deflate"),
            Self::Gzip => write!(f, "gzip"),
            Self::Zstd => write!(f, "zstd"),
        }
    }
}

impl From<CompressionAlgo> for HeaderValue {
    #[inline]
    fn from(algo: CompressionAlgo) -> Self {
        match algo {
            CompressionAlgo::Brotli => Self::from_static("br"),
            CompressionAlgo::Deflate => Self::from_static("deflate"),
            CompressionAlgo::Gzip => Self::from_static("gzip"),
            CompressionAlgo::Zstd => Self::from_static("zstd"),
        }
    }
}

fn append_vary_accept_encoding(headers: &mut HeaderMap) {
    let already_varies = headers
        .get_all(VARY)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|value| {
            let value = value.trim();
            value == "*" || value.eq_ignore_ascii_case("accept-encoding")
        });
    if !already_varies {
        headers.append(VARY, HeaderValue::from_static("Accept-Encoding"));
    }
}

/// Trait for collecting static roots.
pub trait StaticRoots {
    /// Collect all static roots.
    fn collect(self) -> Vec<PathBuf>;
}

impl StaticRoots for &str {
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl StaticRoots for &String {
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl StaticRoots for String {
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl StaticRoots for PathBuf {
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        vec![self]
    }
}
impl<T> StaticRoots for Vec<T>
where
    T: Into<PathBuf> + AsRef<OsStr>,
{
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        self.iter().map(Into::into).collect()
    }
}
impl<T, const N: usize> StaticRoots for [T; N]
where
    T: Into<PathBuf> + AsRef<OsStr>,
{
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        self.iter().map(Into::into).collect()
    }
}

/// Handler that serves static files from directories.
///
/// This handler can serve files from one or more directory paths,
/// with support for directory listing, compressed file variants,
/// and default files.
///
/// # Examples
///
/// ```
/// use salvo_core::prelude::*;
/// use salvo_serve_static::StaticDir;
///
/// let router = Router::new().push(
///     Router::with_path("static/<**>").get(
///         StaticDir::new(["assets", "static"])
///             .defaults("index.html")
///             .auto_list(true),
///     ),
/// );
/// ```
#[non_exhaustive]
pub struct StaticDir {
    /// Static root directories to search for files
    pub roots: Vec<PathBuf>,
    /// Chunk size for file reading (in bytes)
    pub chunk_size: Option<u64>,
    /// Small-file preload threshold for served files (in bytes)
    pub preload_threshold: Option<u64>,
    /// Whether to include dot files (files/directories starting with .)
    pub include_dot_files: bool,
    exclude_filters: Vec<Box<dyn Fn(&str) -> bool + Send + Sync>>,
    /// Whether to automatically list directories when default file isn't found
    pub auto_list: bool,
    /// Map of compression algorithms to file extensions for compressed variants
    pub compressed_variations: HashMap<CompressionAlgo, Vec<String>>,
    /// Default file names to look for in directories (e.g., "index.html")
    pub defaults: Vec<String>,
    /// Fallback file to serve when requested file isn't found
    pub fallback: Option<String>,
    /// Cache of canonicalized roots, revalidated against the current (public,
    /// mutable) `roots` value on every request and re-resolved after a short TTL.
    /// See [`StaticDir::canonical_roots`].
    canonical_cache: RwLock<Option<Arc<CanonicalCache>>>,
    /// Claimed by the single request that re-resolves an expired cache, so a TTL
    /// expiry under concurrent traffic does not stampede `canonicalize` calls.
    canonical_refreshing: AtomicBool,
}
impl Debug for StaticDir {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticDir")
            .field("roots", &self.roots)
            .field("chunk_size", &self.chunk_size)
            .field("preload_threshold", &self.preload_threshold)
            .field("include_dot_files", &self.include_dot_files)
            .field("auto_list", &self.auto_list)
            .field("compressed_variations", &self.compressed_variations)
            .field("defaults", &self.defaults)
            .field("fallback", &self.fallback)
            .finish()
    }
}
impl StaticDir {
    /// Creates a new `StaticDir`.
    #[inline]
    pub fn new<T: StaticRoots + Sized>(roots: T) -> Self {
        let mut compressed_variations = HashMap::new();
        compressed_variations.insert(CompressionAlgo::Brotli, vec!["br".to_owned()]);
        compressed_variations.insert(CompressionAlgo::Zstd, vec!["zst".to_owned()]);
        compressed_variations.insert(CompressionAlgo::Gzip, vec!["gz".to_owned()]);
        compressed_variations.insert(CompressionAlgo::Deflate, vec!["deflate".to_owned()]);

        Self {
            roots: roots.collect(),
            chunk_size: None,
            preload_threshold: None,
            include_dot_files: false,
            exclude_filters: vec![],
            auto_list: false,
            compressed_variations,
            defaults: vec![],
            fallback: None,
            canonical_cache: RwLock::new(None),
            canonical_refreshing: AtomicBool::new(false),
        }
    }

    /// Sets include_dot_files.
    #[inline]
    #[must_use]
    pub fn include_dot_files(mut self, include_dot_files: bool) -> Self {
        self.include_dot_files = include_dot_files;
        self
    }

    /// Exclude files.
    ///
    /// The filter function returns true to exclude the file.
    #[inline]
    #[must_use]
    pub fn exclude<F>(mut self, filter: F) -> Self
    where
        F: Fn(&str) -> bool + Send + Sync + 'static,
    {
        self.exclude_filters.push(Box::new(filter));
        self
    }

    /// Sets auto_list.
    #[inline]
    #[must_use]
    pub fn auto_list(mut self, auto_list: bool) -> Self {
        self.auto_list = auto_list;
        self
    }

    /// Sets compressed_variations.
    #[inline]
    #[must_use]
    pub fn compressed_variation<A>(mut self, algo: A, exts: &str) -> Self
    where
        A: Into<CompressionAlgo>,
    {
        self.compressed_variations.insert(
            algo.into(),
            exts.split(',').map(|s| s.trim().to_owned()).collect(),
        );
        self
    }

    /// Sets defaults.
    #[inline]
    #[must_use]
    pub fn defaults(mut self, defaults: impl IntoVecString) -> Self {
        self.defaults = defaults.into_vec_string();
        self
    }

    /// Sets fallback.
    #[must_use]
    pub fn fallback(mut self, fallback: impl Into<String>) -> Self {
        self.fallback = Some(fallback.into());
        self
    }

    /// During the file chunk read, the maximum read size at one time will affect the
    /// access experience and the demand for server memory.
    ///
    /// This controls streaming chunks and does not change `NamedFile`'s small-file preload
    /// threshold.
    ///
    /// Please set it according to your own situation.
    ///
    /// The default is 1M.
    #[inline]
    #[must_use]
    pub fn chunk_size(mut self, size: u64) -> Self {
        self.chunk_size = Some(size);
        self
    }

    /// Sets the small-file preload threshold.
    ///
    /// Files whose size is less than or equal to this threshold are read during `NamedFile`
    /// construction and sent from memory. Larger files are streamed in chunks. Set this to `0` to
    /// disable preloading for non-empty files.
    #[inline]
    #[must_use]
    pub fn preload_threshold(mut self, threshold: u64) -> Self {
        self.preload_threshold = Some(threshold);
        self
    }

    #[inline]
    fn is_compressed_ext(&self, ext: &str) -> bool {
        for exts in self.compressed_variations.values() {
            if exts.iter().any(|e| e == ext) {
                return true;
            }
        }
        false
    }
}

struct CanonicalRoot {
    path: PathBuf,
    canonical_path: PathBuf,
}

/// How long resolved canonical roots stay fresh before they are re-resolved.
///
/// The TTL bounds how long a root that is (or contains) a retargeted symlink —
/// e.g. a `current -> release-N` deployment layout — can keep serving from the
/// old canonical target: at most one second, at a cost of at most one
/// re-resolution per root per second regardless of request rate.
const CANONICAL_ROOTS_TTL: Duration = Duration::from_secs(1);

/// Canonicalized roots together with the `roots` value they were derived from.
struct CanonicalCache {
    /// The `StaticDir::roots` value this cache was computed from.
    source: Vec<PathBuf>,
    entries: Vec<CanonicalRoot>,
    /// When the roots were resolved; freshness expires after [`CANONICAL_ROOTS_TTL`].
    resolved_at: Instant,
}

impl CanonicalCache {
    /// The cache is usable when it was computed from the current `roots` value,
    /// every root resolved successfully, and the TTL has not elapsed. A root can
    /// fail to canonicalize when its directory does not exist yet (e.g. it is
    /// created after startup); keeping such a cache would hide the root forever,
    /// so re-resolve until all succeed. The TTL bounds staleness for roots whose
    /// canonical target changes without `roots` being touched (symlink retarget).
    fn is_fresh(&self, current_roots: &[PathBuf]) -> bool {
        self.entries.len() == self.source.len()
            && self.resolved_at.elapsed() < CANONICAL_ROOTS_TTL
            && self.source == current_roots
    }
}

struct ResolvedPath {
    path: PathBuf,
    canonical_root: PathBuf,
    metadata: Metadata,
}

impl StaticDir {
    /// Canonicalized roots, cached across requests.
    ///
    /// Canonicalizing every root used to run on each request, costing filesystem
    /// syscalls before any file was served. The result is now cached and revalidated
    /// against the current `roots` value (which is public and mutable) with a cheap
    /// path comparison, and re-resolved after [`CANONICAL_ROOTS_TTL`] so a root whose
    /// canonical target changes without `roots` being touched — e.g. a retargeted
    /// `current -> release-N` deployment symlink — is picked up within the TTL.
    async fn canonical_roots(&self) -> Arc<CanonicalCache> {
        let cached = self
            .canonical_cache
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(cached) = &cached
            && cached.is_fresh(&self.roots)
        {
            return cached.clone();
        }
        // The cache is stale or absent. Only one request re-resolves it; the others
        // keep serving the previous value when it was built from the same `roots`
        // (staleness stays bounded by the refresh duration), so a TTL expiry under
        // concurrent traffic does not stampede `canonicalize` calls.
        let refresh_claimed = self
            .canonical_refreshing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        if !refresh_claimed
            && let Some(cached) = cached
            && cached.source == self.roots
        {
            return cached;
        }
        // Release the claim even if this future is cancelled mid-resolution
        // (e.g. the client disconnects), otherwise refreshes would stop forever.
        struct RefreshGuard<'a>(&'a AtomicBool);
        impl Drop for RefreshGuard<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _guard = refresh_claimed.then(|| RefreshGuard(&self.canonical_refreshing));
        // Reaching here without the claim means the cache cannot be served at all
        // (first request, or `roots` was just mutated); duplicated resolution in
        // that rare window is acceptable.
        let source = self.roots.clone();
        let mut entries = Vec::with_capacity(source.len());
        for root in &source {
            if let Ok(canonical) = tokio::fs::canonicalize(root).await {
                entries.push(CanonicalRoot {
                    path: root.clone(),
                    canonical_path: canonical,
                });
            }
        }
        let fresh = Arc::new(CanonicalCache {
            source,
            entries,
            resolved_at: Instant::now(),
        });
        *self
            .canonical_cache
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(fresh.clone());
        fresh
    }

    async fn resolve_root_path(
        &self,
        root: &CanonicalRoot,
        path: PathBuf,
        apply_exclude_filters: bool,
    ) -> Option<ResolvedPath> {
        if apply_exclude_filters {
            let raw_path = path_slash::PathBufExt::to_slash_lossy(&path);
            if self.exclude_filters.iter().any(|filter| filter(&raw_path)) {
                return None;
            }
        }

        let metadata = tokio::fs::symlink_metadata(&path).await.ok()?;
        let canonical_path = tokio::fs::canonicalize(&path).await.ok()?;
        if !canonical_path.starts_with(&root.canonical_path) {
            // The mismatch means either a symlink-escape attempt or a root whose
            // canonical target changed after being cached (e.g. a retargeted
            // `current -> release-N` deployment symlink). Re-resolve the root once
            // to tell the two apart, so a deploy swap serves the new release
            // immediately instead of returning 404 until the cache TTL expires.
            let fresh_root = tokio::fs::canonicalize(&root.path).await.ok()?;
            if fresh_root == root.canonical_path || !canonical_path.starts_with(&fresh_root) {
                // The root is unchanged (or still does not contain the target):
                // this is a genuine escape attempt.
                return None;
            }
            self.heal_canonical_root(&root.path, &root.canonical_path, fresh_root.clone());
            return Some(ResolvedPath {
                path,
                canonical_root: fresh_root,
                metadata,
            });
        }
        Some(ResolvedPath {
            path,
            canonical_root: root.canonical_path.clone(),
            metadata,
        })
    }

    /// Patch the cached canonical path of `root_path` after detecting that its
    /// canonical target changed, so subsequent requests stop paying the extra
    /// root re-resolution until the TTL refresh.
    fn heal_canonical_root(&self, root_path: &Path, old_canonical: &Path, fresh: PathBuf) {
        let mut guard = self
            .canonical_cache
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(cached) = guard.as_ref() else {
            return;
        };
        // Only patch the cache generation the stale value came from; a concurrent
        // full refresh already has up-to-date data.
        if !cached
            .entries
            .iter()
            .any(|entry| entry.path == root_path && entry.canonical_path == old_canonical)
        {
            return;
        }
        let entries = cached
            .entries
            .iter()
            .map(|entry| CanonicalRoot {
                path: entry.path.clone(),
                canonical_path: if entry.path == root_path && entry.canonical_path == old_canonical
                {
                    fresh.clone()
                } else {
                    entry.canonical_path.clone()
                },
            })
            .collect();
        *guard = Some(Arc::new(CanonicalCache {
            source: cached.source.clone(),
            entries,
            resolved_at: cached.resolved_at,
        }));
    }

    async fn resolve_relative_path(
        &self,
        root: &CanonicalRoot,
        rel_path: impl AsRef<Path>,
    ) -> Option<ResolvedPath> {
        self.resolve_root_path(root, root.path.join(rel_path), true)
            .await
    }

    async fn resolve_child_path(&self, root: &Path, path: PathBuf) -> Option<ResolvedPath> {
        let synthetic_root = CanonicalRoot {
            path: root.to_owned(),
            canonical_path: root.to_owned(),
        };
        // Apply exclude filters here too: default files (e.g. `index.html`) and
        // compressed variants (e.g. `.gz`) must not bypass `exclude_filters`.
        self.resolve_root_path(&synthetic_root, path, true).await
    }
}
#[derive(Serialize, Deserialize, Debug)]
struct CurrentInfo {
    path: String,
    files: Vec<FileInfo>,
    dirs: Vec<DirInfo>,
}
impl CurrentInfo {
    #[inline]
    fn new(path: String, files: Vec<FileInfo>, dirs: Vec<DirInfo>) -> Self {
        Self { path, files, dirs }
    }
}
#[derive(Serialize, Deserialize, Debug)]
struct FileInfo {
    name: String,
    size: u64,
    modified: OffsetDateTime,
}
impl FileInfo {
    #[inline]
    #[must_use]
    fn new(name: String, metadata: &Metadata) -> Self {
        Self {
            name,
            size: metadata.len(),
            modified: metadata
                .modified()
                .unwrap_or_else(|_| SystemTime::now())
                .into(),
        }
    }
}
#[derive(Serialize, Deserialize, Debug)]
struct DirInfo {
    name: String,
    modified: OffsetDateTime,
}
impl DirInfo {
    #[inline]
    fn new(name: String, metadata: &Metadata) -> Self {
        Self {
            name,
            modified: metadata
                .modified()
                .unwrap_or_else(|_| SystemTime::now())
                .into(),
        }
    }
}

#[async_trait]
impl Handler for StaticDir {
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        let req_path = req.uri().path();
        let rel_path = if let Some(rest) = req.params().tail() {
            rest
        } else {
            &*decode_url_path(req_path)
        };
        let rel_path = normalize_url_path(rel_path);
        let mut files: HashMap<String, Metadata> = HashMap::new();
        let mut dirs: HashMap<String, Metadata> = HashMap::new();
        let is_dot_file = Path::new(&rel_path)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.starts_with('.'))
            .unwrap_or(false);
        let mut abs_path = None;
        let roots = self.canonical_roots().await;
        if self.include_dot_files || !is_dot_file {
            for root in &roots.entries {
                // Use a single async symlink_metadata call for file type checks, then verify
                // the canonical target stays under the canonical root before serving it.
                if let Some(path) = self.resolve_relative_path(root, &rel_path).await {
                    if path.metadata.is_dir() {
                        if !req_path.ends_with('/') && !req_path.is_empty() {
                            redirect_to_dir_url(req.uri(), res);
                            return;
                        }

                        for default_file in &self.defaults {
                            let default_path = path.path.join(default_file);
                            let Some(default_path) = self
                                .resolve_child_path(&path.canonical_root, default_path)
                                .await
                            else {
                                continue;
                            };
                            if default_path.metadata.is_file() {
                                abs_path = Some(default_path);
                                break;
                            }
                        }

                        if self.auto_list && abs_path.is_none() {
                            abs_path = Some(path);
                        }
                        if abs_path.is_some() {
                            break;
                        }
                    } else if path.metadata.is_file() {
                        abs_path = Some(path);
                    }
                }
            }
        }
        let fallback = self.fallback.as_deref().unwrap_or_default();
        if abs_path.is_none() && !fallback.is_empty() && is_safe_relative_path(fallback) {
            for root in &roots.entries {
                if let Some(path) = self.resolve_relative_path(root, fallback).await {
                    if !path.metadata.is_file() {
                        continue;
                    }
                    abs_path = Some(path);
                    break;
                }
            }
        }

        let Some(abs_path) = abs_path else {
            res.render(StatusError::not_found());
            return;
        };

        let is_file = abs_path.metadata.is_file();
        let canonical_root = abs_path.canonical_root;
        let abs_path = abs_path.path;

        if is_file {
            let ext = abs_path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase());
            let is_compressed_ext = ext
                .as_deref()
                .map(|ext| self.is_compressed_ext(ext))
                .unwrap_or(false);
            let mut content_encoding = None;
            let mut varies_on_accept_encoding = false;
            let content_type = mime_infer::from_path(&abs_path).first();

            let named_path = if !is_compressed_ext {
                if !self.compressed_variations.is_empty() {
                    let mut new_abs_path = None;
                    let header = req
                        .headers()
                        .get(ACCEPT_ENCODING)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or_default();
                    // Skip compressed variant lookup when client only accepts identity
                    if !header.is_empty() && header != "identity" {
                        varies_on_accept_encoding = true;
                        'accepted_encoding: for (algo, level) in http::parse_accept_encoding(header)
                        {
                            if level == 0 {
                                continue;
                            }
                            if algo.eq_ignore_ascii_case("identity") {
                                break;
                            }
                            let Ok(algo) = algo.parse::<CompressionAlgo>() else {
                                continue;
                            };
                            let Some(exts) = self.compressed_variations.get(&algo) else {
                                continue;
                            };
                            for zip_ext in exts {
                                let mut path = abs_path.clone();
                                path.as_mut_os_string().push(".");
                                path.as_mut_os_string().push(zip_ext.as_str());
                                if self
                                    .resolve_child_path(&canonical_root, path.clone())
                                    .await
                                    .map(|path| path.metadata.is_file())
                                    .unwrap_or(false)
                                {
                                    new_abs_path = Some(path);
                                    content_encoding = Some(algo.to_string());
                                    break 'accepted_encoding;
                                }
                            }
                        }
                    }
                    new_abs_path.unwrap_or(abs_path)
                } else {
                    abs_path
                }
            } else {
                abs_path
            };

            let (builder, varies_on_accept_encoding) = {
                let mut builder = NamedFile::builder(named_path);
                if let Some(content_encoding) = content_encoding {
                    builder = builder.content_encoding(content_encoding);
                }
                if let Some(size) = self.chunk_size {
                    builder = builder.buffer_size(size);
                }
                if let Some(threshold) = self.preload_threshold {
                    builder = builder.preload_threshold(threshold);
                }
                if let Some(content_type) = content_type {
                    builder = builder.content_type(content_type);
                }
                (builder, varies_on_accept_encoding)
            };
            if let Ok(named_file) = builder.build().await {
                let headers = req.headers();
                named_file.send(headers, res).await;
                if varies_on_accept_encoding {
                    append_vary_accept_encoding(res.headers_mut());
                }
            } else {
                res.render(StatusError::internal_server_error().brief("read file failed"));
            }
        } else if !is_file {
            // list the dir
            if let Ok(mut entries) = tokio::fs::read_dir(&abs_path).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let file_name = entry.file_name().to_string_lossy().into_owned();
                    if self.include_dot_files || !file_name.starts_with('.') {
                        let raw_path = join_path!(&abs_path, &file_name);
                        if self.exclude_filters.iter().any(|filter| filter(&raw_path)) {
                            continue;
                        }
                        if let Ok(metadata) = entry.metadata().await {
                            if metadata.is_dir() {
                                dirs.entry(file_name).or_insert(metadata);
                            } else {
                                files.entry(file_name).or_insert(metadata);
                            }
                        }
                    }
                }
            }

            let format = req.first_accept().unwrap_or(mime::TEXT_HTML);
            let mut files: Vec<FileInfo> = files
                .into_iter()
                .map(|(name, metadata)| FileInfo::new(name, &metadata))
                .collect();
            files.sort_by(|a, b| a.name.cmp(&b.name));
            let mut dirs: Vec<DirInfo> = dirs
                .into_iter()
                .map(|(name, metadata)| DirInfo::new(name, &metadata))
                .collect();
            dirs.sort_by(|a, b| a.name.cmp(&b.name));
            let root = CurrentInfo::new(decode_url_path(req_path), files, dirs);
            res.status_code(StatusCode::OK);
            match format.subtype().as_ref() {
                "plain" => res.render(Text::Plain(list_text(&root))),
                "json" => res.render(Text::Json(list_json(&root))),
                "xml" => res.render(Text::Xml(list_xml(&root))),
                _ => res.render(Text::Html(list_html(&root))),
            };
        }
    }
}

#[inline]
fn list_json(current: &CurrentInfo) -> String {
    json!(current).to_string()
}
fn list_xml(current: &CurrentInfo) -> String {
    let mut xml = "<list>".to_owned();
    if current.dirs.is_empty() && current.files.is_empty() {
        xml.push_str("No files");
    } else {
        let format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
        for dir in &current.dirs {
            let _ = write!(
                xml,
                "<dir><name>{}</name><modified>{}</modified><link>{}</link></dir>",
                xml_escape(&dir.name),
                dir.modified.format(&format).expect("format time failed"),
                encode_url_path(&dir.name),
            );
        }
        for file in &current.files {
            let _ = write!(
                xml,
                "<file><name>{}</name><modified>{}</modified><size>{}</size><link>{}</link></file>",
                xml_escape(&file.name),
                file.modified.format(&format).expect("format time failed"),
                file.size,
                encode_url_path(&file.name),
            );
        }
    }
    xml.push_str("</list>");
    xml
}

fn is_safe_relative_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn human_size(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let mut index = 0;
    let mut bytes = bytes as f64;

    while bytes >= 1024.0 && index < units.len() - 1 {
        bytes /= 1024.0;
        index += 1;
    }

    bytes = (bytes * 100.0).round() / 100.0;
    if bytes == 1024.0 && index < units.len() - 1 {
        index += 1;
        bytes = 1.0;
    }
    format!("{} {}", bytes, units[index])
}
fn list_html(current: &CurrentInfo) -> String {
    fn header_link(out: &mut String, path: &str) {
        let _ = write!(out, r#"<a href="/">{HOME_ICON}</a>"#);
        let mut link = String::new();
        for seg in path
            .trim_start_matches('/')
            .trim_end_matches('/')
            .split('/')
        {
            let encoded = encode_url_path(seg);
            link.push('/');
            link.push_str(&encoded);
            let _ = write!(out, r#"/<a href="{link}">{encoded}</a>"#);
        }
    }
    let mut html = format!(
        r#"<!DOCTYPE html><html><head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width">
        <title>{}</title>
        <style>{}</style></head><body><header><h3>Index of: "#,
        encode_url_path(&current.path),
        HTML_STYLE,
    );
    header_link(&mut html, &current.path);
    let _ = write!(html, "</h3></header><hr/>");
    if current.dirs.is_empty() && current.files.is_empty() {
        let _ = write!(html, "<p>No files</p>");
    } else {
        let _ = write!(html, "<table><tr><th>");
        if !(current.path.is_empty() || current.path == "/") {
            let _ = write!(html, "<a href=\"../\">[..]</a>");
        }
        let _ = write!(
            html,
            "</th><th>Name</th><th>Last modified</th><th>Size</th></tr>"
        );
        let format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
        for dir in &current.dirs {
            let encoded = encode_url_path(&dir.name);
            let _ = write!(
                html,
                r#"<tr><td>{}</td><td><a href="./{}/">{}</a></td><td>{}</td><td></td></tr>"#,
                DIR_ICON,
                encoded,
                encoded,
                dir.modified.format(&format).expect("format time failed"),
            );
        }
        for file in &current.files {
            let encoded = encode_url_path(&file.name);
            let _ = write!(
                html,
                r#"<tr><td>{}</td><td><a href="./{}">{}</a></td><td>{}</td><td>{}</td></tr>"#,
                FILE_ICON,
                encoded,
                encoded,
                file.modified.format(&format).expect("format time failed"),
                human_size(file.size)
            );
        }
        let _ = write!(html, "</table>");
    }
    let _ = write!(
        html,
        r#"<hr/><footer><a href="https://salvo.rs" target="_blank">salvo</a></footer></body>"#
    );
    html
}
fn list_text(current: &CurrentInfo) -> String {
    use std::fmt::Write;
    let format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    let mut txt = format!("Directory: {}\n\n", current.path);
    for dir in &current.dirs {
        let _ = writeln!(
            txt,
            "[DIR]  {}  {}",
            dir.modified.format(&format).expect("format time failed"),
            dir.name,
        );
    }
    for file in &current.files {
        let _ = writeln!(
            txt,
            "{:>10}  {}  {}",
            human_size(file.size),
            file.modified.format(&format).expect("format time failed"),
            file.name,
        );
    }
    txt
}

const HTML_STYLE: &str = r#"
    :root {
        --bg-color: #fff;
        --text-color: #222;
        --link-color: #0366d6;
        --link-visited-color: #f22526;
        --dir-icon-color: #79b8ff;
        --file-icon-color: #959da5;
    }
    body {background: var(--bg-color); color: var(--text-color);}
    a {text-decoration:none;color:var(--link-color);}
    a:visited {color: var(--link-visited-color);}
    a:hover {text-decoration:underline;}
    header a {padding: 0 6px;}
    footer {text-align:center;font-size:12px;}
    table {text-align:left;border-collapse: collapse;}
    tr {border-bottom: solid 1px #ccc;}
    tr:last-child {border-bottom: none;}
    th, td {padding: 5px;}
    th:first-child,td:first-child {text-align: center;}
    svg[data-icon="dir"] {vertical-align: text-bottom; color: var(--dir-icon-color); fill: currentColor;}
    svg[data-icon="file"] {vertical-align: text-bottom; color: var(--file-icon-color); fill: currentColor;}
    svg[data-icon="home"] {width:18px;}
    @media (prefers-color-scheme: dark) {
        :root {
            --bg-color: #222;
            --text-color: #ddd;
            --link-color: #539bf5;
            --link-visited-color: #f25555;
            --dir-icon-color: #7da3d0;
            --file-icon-color: #545d68;
        }
    }"#;
const DIR_ICON: &str = r#"<svg aria-label="Directory" data-icon="dir" width="20" height="20" viewBox="0 0 512 512" version="1.1" role="img"><path fill="currentColor" d="M464 128H272l-64-64H48C21.49 64 0 85.49 0 112v288c0 26.51 21.49 48 48 48h416c26.51 0 48-21.49 48-48V176c0-26.51-21.49-48-48-48z"></path></svg>"#;
const FILE_ICON: &str = r#"<svg aria-label="File" data-icon="file" width="20" height="20" viewBox="0 0 384 512" version="1.1" role="img"><path d="M369.9 97.9L286 14C277 5 264.8-.1 252.1-.1H48C21.5 0 0 21.5 0 48v416c0 26.5 21.5 48 48 48h288c26.5 0 48-21.5 48-48V131.9c0-12.7-5.1-25-14.1-34zM332.1 128H256V51.9l76.1 76.1zM48 464V48h160v104c0 13.3 10.7 24 24 24h104v288H48z"/></svg>"#;
const HOME_ICON: &str = r#"<svg aria-hidden="true" data-icon="home" viewBox="0 0 576 512"><path fill="currentColor" d="M280.37 148.26L96 300.11V464a16 16 0 0 0 16 16l112.06-.29a16 16 0 0 0 15.92-16V368a16 16 0 0 1 16-16h64a16 16 0 0 1 16 16v95.64a16 16 0 0 0 16 16.05L464 480a16 16 0 0 0 16-16V300L295.67 148.26a12.19 12.19 0 0 0-15.3 0zM571.6 251.47L488 182.56V44.05a12 12 0 0 0-12-12h-56a12 12 0 0 0-12 12v72.61L318.47 43a48 48 0 0 0-61 0L4.34 251.47a12 12 0 0 0-1.6 16.9l25.5 31A12 12 0 0 0 45.15 301l235.22-193.74a12.19 12.19 0 0 1 15.3 0L530.9 301a12 12 0 0 0 16.9-1.6l25.5-31a12 12 0 0 0-1.7-16.93z"></path></svg>"#;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::dir::{StaticDir, human_size, is_safe_relative_path, xml_escape};

    #[tokio::test]
    async fn test_canonical_roots_cached_and_invalidated() {
        let dir_a = tempfile::tempdir().expect("create temp dir");
        let dir_b = tempfile::tempdir().expect("create temp dir");

        let mut static_dir = StaticDir::new(dir_a.path().to_path_buf());

        // First call resolves and caches; second call must return the same Arc.
        let first = static_dir.canonical_roots().await;
        assert_eq!(first.entries.len(), 1);
        let second = static_dir.canonical_roots().await;
        assert!(Arc::ptr_eq(&first, &second), "fresh cache should be reused");

        // Mutating the public `roots` field must invalidate the cache.
        static_dir.roots = vec![dir_b.path().to_path_buf()];
        let third = static_dir.canonical_roots().await;
        assert!(!Arc::ptr_eq(&second, &third));
        assert_eq!(third.entries.len(), 1);
        assert_eq!(third.entries[0].path, dir_b.path());
    }

    #[tokio::test]
    async fn test_canonical_roots_retries_missing_root() {
        let parent = tempfile::tempdir().expect("create temp dir");
        let missing: PathBuf = parent.path().join("created-later");

        let static_dir = StaticDir::new(missing.clone());

        // The root does not exist yet, so it cannot be canonicalized...
        let before = static_dir.canonical_roots().await;
        assert!(before.entries.is_empty());

        // ...and an incomplete resolution must not be cached: once the directory
        // exists, the next request must pick it up.
        std::fs::create_dir(&missing).expect("create root dir");
        let after = static_dir.canonical_roots().await;
        assert_eq!(after.entries.len(), 1);
        assert_eq!(after.entries[0].path, missing);
    }

    /// Age the currently cached canonical roots so the TTL freshness check fails.
    fn expire_canonical_cache(static_dir: &StaticDir) {
        let mut guard = static_dir
            .canonical_cache
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let cached = guard.take().expect("cache should be populated");
        let expired = std::time::Instant::now()
            .checked_sub(super::CANONICAL_ROOTS_TTL * 2)
            .expect("process uptime exceeds twice the TTL");
        *guard = Some(Arc::new(super::CanonicalCache {
            source: cached.source.clone(),
            entries: cached
                .entries
                .iter()
                .map(|entry| super::CanonicalRoot {
                    path: entry.path.clone(),
                    canonical_path: entry.canonical_path.clone(),
                })
                .collect(),
            resolved_at: expired,
        }));
    }

    #[tokio::test]
    async fn test_canonical_roots_reresolved_after_ttl() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let static_dir = StaticDir::new(dir.path().to_path_buf());

        let first = static_dir.canonical_roots().await;
        expire_canonical_cache(&static_dir);

        // An expired cache must be re-resolved even though `roots` is unchanged.
        let second = static_dir.canonical_roots().await;
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second.entries.len(), 1);
    }

    #[tokio::test]
    async fn test_canonical_roots_stale_while_revalidate() {
        use std::sync::atomic::Ordering;

        let dir_a = tempfile::tempdir().expect("create temp dir");
        let dir_b = tempfile::tempdir().expect("create temp dir");
        let mut static_dir = StaticDir::new(dir_a.path().to_path_buf());

        let _ = static_dir.canonical_roots().await;
        expire_canonical_cache(&static_dir);
        let stale = static_dir
            .canonical_cache
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .expect("cache should be populated");

        // While another request holds the refresh claim, an expired cache built
        // from the same `roots` keeps being served instead of stampeding.
        static_dir.canonical_refreshing.store(true, Ordering::Release);
        let served = static_dir.canonical_roots().await;
        assert!(Arc::ptr_eq(&stale, &served));

        // But a cache built from *different* roots must never be served stale:
        // it is re-resolved even while the refresh claim is held elsewhere.
        static_dir.roots = vec![dir_b.path().to_path_buf()];
        let served = static_dir.canonical_roots().await;
        assert_eq!(served.entries[0].path, dir_b.path());
        // The non-claiming resolution must not release the foreign claim.
        assert!(static_dir.canonical_refreshing.load(Ordering::Acquire));
        static_dir
            .canonical_refreshing
            .store(false, Ordering::Release);

        // With the claim released, a later request can refresh normally again.
        expire_canonical_cache(&static_dir);
        let refreshed = static_dir.canonical_roots().await;
        assert!(!Arc::ptr_eq(&served, &refreshed));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_resolve_heals_retargeted_symlink_root_immediately() {
        // A deploy swap (`current -> release-a` retargeted to `release-b`) must be
        // served immediately — not 404 until the cache TTL expires — while a
        // genuine escape attempt is still rejected.
        let parent = tempfile::tempdir().expect("create temp dir");
        let release_a = parent.path().join("release-a");
        let release_b = parent.path().join("release-b");
        let current = parent.path().join("current");
        std::fs::create_dir(&release_a).expect("create release-a");
        std::fs::create_dir(&release_b).expect("create release-b");
        std::fs::write(release_b.join("app.js"), b"new release").expect("write file");
        std::fs::write(parent.path().join("secret.txt"), b"secret").expect("write file");
        std::os::unix::fs::symlink(&release_a, &current).expect("create symlink");

        let static_dir = StaticDir::new(current.clone());
        let cache = static_dir.canonical_roots().await;

        // Retarget the deployment symlink *without* touching `roots` or the cache.
        std::fs::remove_file(&current).expect("remove symlink");
        std::os::unix::fs::symlink(&release_b, &current).expect("retarget symlink");

        // Resolution against the stale cached root must heal and serve the file.
        let resolved = static_dir
            .resolve_relative_path(&cache.entries[0], "app.js")
            .await
            .expect("retargeted root should heal, not 404");
        let fresh_canonical = std::fs::canonicalize(&release_b).expect("canonicalize release-b");
        assert_eq!(resolved.canonical_root, fresh_canonical);

        // The cache entry was patched in place for subsequent requests.
        let healed = static_dir.canonical_roots().await;
        assert_eq!(healed.entries[0].canonical_path, fresh_canonical);

        // An escape attempt out of the (healed) root is still rejected.
        std::os::unix::fs::symlink(parent.path().join("secret.txt"), release_b.join("leak"))
            .expect("create escape symlink");
        assert!(
            static_dir
                .resolve_relative_path(&healed.entries[0], "leak")
                .await
                .is_none()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_canonical_roots_pick_up_symlink_retarget() {
        // The `current -> release-N` deployment layout: retargeting the symlink
        // must be picked up once the TTL elapses, without touching `roots`.
        let parent = tempfile::tempdir().expect("create temp dir");
        let release_a = parent.path().join("release-a");
        let release_b = parent.path().join("release-b");
        let current = parent.path().join("current");
        std::fs::create_dir(&release_a).expect("create release-a");
        std::fs::create_dir(&release_b).expect("create release-b");
        std::os::unix::fs::symlink(&release_a, &current).expect("create symlink");

        let static_dir = StaticDir::new(current.clone());
        let before = static_dir.canonical_roots().await;
        assert_eq!(
            before.entries[0].canonical_path,
            std::fs::canonicalize(&release_a).expect("canonicalize release-a")
        );

        std::fs::remove_file(&current).expect("remove symlink");
        std::os::unix::fs::symlink(&release_b, &current).expect("retarget symlink");
        expire_canonical_cache(&static_dir);

        let after = static_dir.canonical_roots().await;
        assert_eq!(
            after.entries[0].canonical_path,
            std::fs::canonicalize(&release_b).expect("canonicalize release-b")
        );
    }

    #[tokio::test]
    async fn test_convert_bytes_to_units() {
        assert_eq!("94.03 MB", human_size(98595176)); // 98.59 MB

        let unit = 1024;
        assert_eq!("1 KB", human_size(unit));
        assert_eq!("1023 B", human_size(unit - 1));

        assert_eq!("1 MB", human_size(unit * unit));
        assert_eq!("1 MB", human_size(unit * unit - 1));
        assert_eq!("1023.99 KB", human_size(unit * unit - 10));

        assert_eq!("1 GB", human_size(unit * unit * unit));
        assert_eq!("1 GB", human_size(unit * unit * unit - 1));

        assert_eq!("1 TB", human_size(unit * unit * unit * unit));
        assert_eq!("1 TB", human_size(unit * unit * unit * unit - 1));

        assert_eq!("1 PB", human_size(unit * unit * unit * unit * unit));
        assert_eq!("1 PB", human_size(unit * unit * unit * unit * unit - 1));
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(
            xml_escape(r#"<script a="b">'&</script>"#),
            "&lt;script a=&quot;b&quot;&gt;&apos;&amp;&lt;/script&gt;"
        );
    }

    #[test]
    fn test_fallback_path_must_stay_relative() {
        assert!(is_safe_relative_path("index.html"));
        assert!(is_safe_relative_path("./spa/index.html"));
        assert!(!is_safe_relative_path("../secret.html"));
        assert!(!is_safe_relative_path("spa/../../secret.html"));
        assert!(!is_safe_relative_path("/var/www/index.html"));
    }
}
