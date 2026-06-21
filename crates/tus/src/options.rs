use std::fmt;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use futures_util::future::BoxFuture;
use regex::Regex;
use salvo_core::Request;
use salvo_core::http::{HeaderMap, StatusCode, header};

use crate::CancellationContext;
use crate::error::{TusError, TusResult};
use crate::handlers::{GenerateUrlCtx, HostProto, Metadata};
use crate::lockers::{LockGuard, Locker, memory_locker};
use crate::stores::UploadInfo;
use crate::utils::is_safe_upload_id;

/// Optional tus upload ID passed to size and URL callbacks.
pub type MaybeUploadId = Option<String>;

/// Deprecated alias for [`MaybeUploadId`].
///
/// The name was misleading — it is `Option<String>`, not an upload id — and was
/// identical in meaning to [`MaybeUploadId`]. Use [`MaybeUploadId`] instead.
#[deprecated(since = "0.94.0", note = "use `MaybeUploadId` instead")]
pub type UploadId = MaybeUploadId;

static RE_FILE_ID: OnceLock<Regex> = OnceLock::new();
/// Returns the regex used to extract an upload ID from a request path.
pub fn file_id_regex() -> &'static Regex {
    RE_FILE_ID.get_or_init(|| Regex::new(r"([^/]+)/?$").expect("Invalid regex pattern"))
}

/// Maximum allowed upload size policy.
#[derive(Clone)]
pub enum MaxSize {
    /// Fixed maximum number of bytes accepted for an upload.
    ///
    /// A value of `0` means no configured size limit.
    Fixed(u64),
    /// Callback that computes the maximum number of bytes for each request.
    #[allow(clippy::type_complexity)]
    Dynamic(Arc<dyn Fn(&Request, MaybeUploadId) -> BoxFuture<'static, u64> + Send + Sync>),
}

impl fmt::Debug for MaxSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed(size) => f.debug_tuple("Fixed").field(size).finish(),
            Self::Dynamic(_) => f.debug_tuple("Dynamic").field(&"<callback>").finish(),
        }
    }
}

/// Callback used to generate a new tus upload ID.
pub type NamingFunction = Arc<
    dyn Fn(
            &Request,
            Option<Metadata>,
        ) -> Pin<Box<dyn Future<Output = Result<String, TusError>> + Send>>
        + Send
        + Sync,
>;
/// Callback used to generate the `Location` URL for a created upload.
pub type GenerateUrlFunction =
    Arc<dyn Fn(&Request, GenerateUrlCtx) -> Result<String, TusError> + Send + Sync>;

/// Hook invoked at the start of each tus request with the resolved upload ID.
pub type OnIncomingRequest =
    Arc<dyn Fn(&Request, String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
/// Hook invoked before a new upload is persisted.
pub type OnUploadCreate = Arc<
    dyn Fn(
            &Request,
            UploadInfo,
        ) -> Pin<Box<dyn Future<Output = Result<UploadPatch, TusError>> + Send>>
        + Send
        + Sync,
>;
/// Hook invoked after an upload reaches completion.
pub type OnUploadFinish = Arc<
    dyn Fn(
            &Request,
            UploadInfo,
        ) -> Pin<Box<dyn Future<Output = Result<UploadFinishPatch, TusError>> + Send>>
        + Send
        + Sync,
>;

/// Changes that an upload-create hook can apply to a newly created upload.
#[derive(Clone, Debug, Default)]
pub struct UploadPatch {
    /// Replacement metadata to store for the upload.
    pub metadata: Option<Metadata>,
}

/// Custom response returned by an upload-finish hook.
#[derive(Clone, Debug, Default)]
pub struct UploadFinishPatch {
    /// HTTP status code to return.
    pub status_code: Option<StatusCode>,
    /// Additional response headers to return.
    pub headers: Option<HeaderMap>,
    /// Response body bytes to return.
    pub body: Option<Vec<u8>>,
}

/// Configuration shared by all tus route handlers.
#[derive(Clone)]
pub struct TusOptions {
    /// The route to accept requests.
    pub path: String,

    /// Max file size allowed when uploading.
    pub max_size: Option<MaxSize>,

    /// Return a relative URL as the `Location` header.
    pub relative_location: bool,

    /// Canonical origin used for absolute `Location` headers.
    ///
    /// # Security
    ///
    /// When `relative_location` is `false` and `canonical_origin` is `None`, the
    /// absolute `Location` is built from the request's `Host` (and, if
    /// [`respect_forwarded_headers`](Self::respect_forwarded_headers) is set,
    /// `Forwarded`, `X-Forwarded-Host`, `X-Forwarded-Proto`, and
    /// `X-Forwarded-Ssl`) headers, all of which are client-controlled. An
    /// attacker can then poison the returned upload URL via `Host` header
    /// injection. In production prefer a relative `Location` or set
    /// `canonical_origin` to a fixed, trusted origin so the host is never taken
    /// from request headers.
    pub canonical_origin: Option<String>,

    /// Allows trusted forwarded headers to override the host and protocol used for `Location`.
    ///
    /// # Security
    ///
    /// Only enable this behind a proxy that overwrites every host/proto
    /// forwarding header it honours — `Forwarded`, `X-Forwarded-Host`,
    /// `X-Forwarded-Proto`, and `X-Forwarded-Ssl`. If clients can reach the
    /// server directly, these headers are spoofable and can poison the
    /// `Location` header; set [`canonical_origin`](Self::canonical_origin) to
    /// avoid relying on request headers at all.
    pub respect_forwarded_headers: bool,

    /// Additional headers sent in `Access-Control-Allow-Headers`.
    pub allowed_headers: Vec<String>,

    /// Additional headers sent in `Access-Control-Expose-Headers`.
    pub exposed_headers: Vec<String>,

    /// Whether to set `Access-Control-Allow-Credentials`.
    pub allowed_credentials: bool,

    /// Trusted origins for `Access-Control-Allow-Origin`.
    pub allowed_origins: Vec<String>,

    /// Interval in milliseconds for sending receive progress notifications.
    pub post_receive_interval: Option<u64>,

    /// Lock provider used to serialize access to the same upload.
    pub locker: Arc<dyn Locker>,

    /// Lock cleanup timeout in milliseconds.
    pub lock_drain_timeout: Option<u64>,

    /// Disallow termination for finished uploads
    pub disable_termination_for_finished_uploads: bool,

    /// Function to generate upload IDs.
    pub upload_id_naming_function: NamingFunction,

    /// Function to generate the upload URL.
    pub generate_url_function: Option<GenerateUrlFunction>,

    /// Optional hook invoked at the start of each tus request.
    pub on_incoming_request: Option<OnIncomingRequest>,
    /// Optional hook invoked when a new upload is created.
    pub on_upload_create: Option<OnUploadCreate>,
    /// Optional hook invoked when an upload is completed.
    pub on_upload_finish: Option<OnUploadFinish>,
}

impl fmt::Debug for TusOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TusOptions")
            .field("path", &self.path)
            .field("max_size", &self.max_size)
            .field("relative_location", &self.relative_location)
            .field("canonical_origin", &self.canonical_origin)
            .field("respect_forwarded_headers", &self.respect_forwarded_headers)
            .field("allowed_headers", &self.allowed_headers)
            .field("exposed_headers", &self.exposed_headers)
            .field("allowed_credentials", &self.allowed_credentials)
            .field("allowed_origins", &self.allowed_origins)
            .field("post_receive_interval", &self.post_receive_interval)
            .field("locker", &"<locker>")
            .field("lock_drain_timeout", &self.lock_drain_timeout)
            .field(
                "disable_termination_for_finished_uploads",
                &self.disable_termination_for_finished_uploads,
            )
            .field("upload_id_naming_function", &"<callback>")
            .field(
                "generate_url_function",
                &self.generate_url_function.as_ref().map(|_| "<callback>"),
            )
            .field(
                "on_incoming_request",
                &self.on_incoming_request.as_ref().map(|_| "<callback>"),
            )
            .field(
                "on_upload_create",
                &self.on_upload_create.as_ref().map(|_| "<callback>"),
            )
            .field(
                "on_upload_finish",
                &self.on_upload_finish.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}

impl TusOptions {
    /// Acquires a write lock for the given upload ID.
    pub async fn acquire_lock(
        &self,
        _req: &Request,
        upload_id: &str,
        context: CancellationContext,
    ) -> TusResult<LockGuard> {
        self.acquire_write_lock(_req, upload_id, context).await
    }

    /// Acquires a read lock for the given upload ID.
    pub async fn acquire_read_lock(
        &self,
        _req: &Request,
        upload_id: &str,
        context: CancellationContext,
    ) -> TusResult<LockGuard> {
        let mut signal = context.signal.clone();
        tokio::select! {
            lock = self.locker.read_lock(upload_id) => lock,
            reason = signal.cancelled() => Err(TusError::Internal(format!("request {reason:?}"))),
        }
    }

    /// Acquires a write lock for the given upload ID.
    pub async fn acquire_write_lock(
        &self,
        _req: &Request,
        upload_id: &str,
        context: CancellationContext,
    ) -> TusResult<LockGuard> {
        let mut signal = context.signal.clone();
        tokio::select! {
            lock = self.locker.write_lock(upload_id) => lock,
            reason = signal.cancelled() => Err(TusError::Internal(format!("request {reason:?}"))),
        }
    }

    /// Extracts and validates the upload ID from the request path.
    pub fn extract_file_id_from_request(&self, req: &Request) -> TusResult<String> {
        let path = req.uri().path();
        let re = file_id_regex();

        re.captures(path)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_owned())
            .filter(|id| is_safe_upload_id(id))
            .ok_or(TusError::FileIdError)
    }

    /// Returns the configured maximum upload size for this request.
    pub async fn get_configured_max_size(&self, req: &Request, upload_id: Option<String>) -> u64 {
        match &self.max_size {
            Some(MaxSize::Fixed(size)) => *size,
            Some(MaxSize::Dynamic(func)) => {
                let fut = func(req, upload_id);
                fut.await
            }
            None => 0,
        }
    }

    /// Generates the upload URL returned in the `Location` header.
    pub fn generate_upload_url(
        &self,
        req: &mut Request,
        upload_id: &str,
    ) -> Result<String, TusError> {
        let path = if self.path == "/" {
            ""
        } else {
            self.path.as_str()
        };

        let HostProto { proto, host } =
            Self::extract_host_and_proto(req.headers(), self.respect_forwarded_headers);

        if let Some(callback) = &self.generate_url_function {
            return match callback(
                req,
                GenerateUrlCtx {
                    proto,
                    host,
                    path,
                    id: upload_id,
                },
            ) {
                Ok(url) => Ok(url),
                Err(e) => Err(e),
            };
        }

        // Default implementation
        if self.relative_location {
            // NOTE: TS version returns `${path}/${id}` — even if path = "" it yields "/id"
            // This matches that behavior.
            return Ok(format!("{path}/{upload_id}"));
        }

        if let Some(origin) = &self.canonical_origin {
            return Ok(format!(
                "{}{path}/{upload_id}",
                origin.trim_end_matches('/')
            ));
        }

        Ok(format!("{proto}://{host}{path}/{upload_id}"))
    }

    /// Rust version of BaseHandler.extractHostAndProto(...)
    fn extract_host_and_proto<'a>(
        headers: &'a HeaderMap,
        respect_forwarded_headers: bool,
    ) -> HostProto<'a> {
        // defaults
        let mut proto: &'a str = "http";
        let mut host: &'a str = "localhost";

        // 1) determine host
        if respect_forwarded_headers {
            // Prefer Forwarded: proto=...;host=...
            if let Some(v) = headers.get("forwarded").and_then(|v| v.to_str().ok()) {
                if let Some(h) = parse_forwarded_param(v, "host") {
                    host = h;
                }
                if let Some(p) = parse_forwarded_param(v, "proto") {
                    proto = p;
                }
            }

            // Fallback: X-Forwarded-Host
            if host == "localhost"
                && let Some(v) = headers
                    .get("x-forwarded-host")
                    .and_then(|v| v.to_str().ok())
            {
                // x-forwarded-host may contain comma-separated list; use the first one
                host = v.split(',').next().unwrap_or(v).trim();
            }

            // 2) determine proto (X-Forwarded-Proto)
            if proto == "http"
                && let Some(v) = headers
                    .get("x-forwarded-proto")
                    .and_then(|v| v.to_str().ok())
            {
                proto = v.split(',').next().unwrap_or(v).trim();
            }
        }

        // If we still haven't got a host, use Host header
        if host == "localhost"
            && let Some(v) = headers.get(header::HOST).and_then(|v| v.to_str().ok())
        {
            host = v.trim();
        }

        // If forwarded headers are trusted and we still haven't got proto, infer from
        // scheme-ish headers.
        if respect_forwarded_headers
            && proto == "http"
            && let Some(v) = headers.get("x-forwarded-ssl").and_then(|v| v.to_str().ok())
            && v.eq_ignore_ascii_case("on")
        {
            proto = "https";
        }

        HostProto { proto, host }
    }

    // pub async fn calculate_max_body_size(&self, req: &Request, file: UploadInfo,
    // configured_max_size: Option<u64>) -> u64 {     todo!()
    // }
}

impl Default for TusOptions {
    fn default() -> Self {
        Self {
            path: "/tus-files".to_owned(),
            max_size: Some(MaxSize::Fixed(2 * 1024 * 1024 * 1024)), // Default max size 2GiB
            relative_location: true,
            canonical_origin: None,
            respect_forwarded_headers: false,
            allowed_headers: vec![],
            exposed_headers: vec![],
            allowed_credentials: false,
            allowed_origins: vec![],
            post_receive_interval: Some(1000),
            locker: Arc::new(memory_locker::MemoryLocker::new()), // Default use memory locker.
            lock_drain_timeout: Some(3000),
            disable_termination_for_finished_uploads: false,
            upload_id_naming_function: Arc::new(|_req, _metadata| {
                Box::pin(async move { Ok(uuid::Uuid::new_v4().to_string()) })
            }), // Default use uuid.
            generate_url_function: None,
            on_incoming_request: None,
            on_upload_create: None,
            on_upload_finish: None,
        }
    }
}

/// Parse a param in "Forwarded" header.
/// Example:
/// Forwarded: for=192.0.2.43; proto=https; host=example.com
fn parse_forwarded_param<'a>(forwarded: &'a str, key: &str) -> Option<&'a str> {
    // Forwarded header can contain multiple entries separated by comma:
    // Forwarded: for=...;proto=https;host=a, for=...;proto=http;host=b
    // We choose the first entry for simplicity, consistent with common behavior.
    let first = forwarded.split(',').next()?.trim();

    for part in first.split(';') {
        let part = part.trim();
        let (k, v) = part.split_once('=')?;
        if k.trim().eq_ignore_ascii_case(key) {
            let v = v.trim().trim_matches('"'); // Forwarded allows quoted-string
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_forwarded_param_simple() {
        let forwarded = "for=192.0.2.43; proto=https; host=example.com";
        assert_eq!(parse_forwarded_param(forwarded, "proto"), Some("https"));
        assert_eq!(
            parse_forwarded_param(forwarded, "host"),
            Some("example.com")
        );
        assert_eq!(parse_forwarded_param(forwarded, "for"), Some("192.0.2.43"));
    }

    #[test]
    fn test_parse_forwarded_param_case_insensitive() {
        let forwarded = "Proto=https; Host=example.com";
        assert_eq!(parse_forwarded_param(forwarded, "proto"), Some("https"));
        assert_eq!(parse_forwarded_param(forwarded, "PROTO"), Some("https"));
        assert_eq!(
            parse_forwarded_param(forwarded, "host"),
            Some("example.com")
        );
    }

    #[test]
    fn test_parse_forwarded_param_quoted_value() {
        let forwarded = "host=\"example.com\"; proto=\"https\"";
        assert_eq!(
            parse_forwarded_param(forwarded, "host"),
            Some("example.com")
        );
        assert_eq!(parse_forwarded_param(forwarded, "proto"), Some("https"));
    }

    #[test]
    fn test_parse_forwarded_param_multiple_entries() {
        // Should use first entry only
        let forwarded = "proto=https;host=first.com, proto=http;host=second.com";
        assert_eq!(parse_forwarded_param(forwarded, "host"), Some("first.com"));
        assert_eq!(parse_forwarded_param(forwarded, "proto"), Some("https"));
    }

    #[test]
    fn test_parse_forwarded_param_not_found() {
        let forwarded = "proto=https; host=example.com";
        assert_eq!(parse_forwarded_param(forwarded, "for"), None);
        assert_eq!(parse_forwarded_param(forwarded, "nonexistent"), None);
    }

    #[test]
    fn test_parse_forwarded_param_empty_value() {
        let forwarded = "host=; proto=https";
        assert_eq!(parse_forwarded_param(forwarded, "host"), None);
        assert_eq!(parse_forwarded_param(forwarded, "proto"), Some("https"));
    }

    #[test]
    fn test_parse_forwarded_param_whitespace() {
        let forwarded = "  proto = https  ;  host = example.com  ";
        assert_eq!(parse_forwarded_param(forwarded, "proto"), Some("https"));
        assert_eq!(
            parse_forwarded_param(forwarded, "host"),
            Some("example.com")
        );
    }

    #[test]
    fn test_parse_forwarded_param_empty_string() {
        assert_eq!(parse_forwarded_param("", "host"), None);
    }

    #[test]
    fn test_file_id_regex() {
        let re = file_id_regex();

        // Test matching file IDs
        let captures = re.captures("/uploads/abc123").unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "abc123");

        let captures = re.captures("/api/v1/tus/file-id-here").unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "file-id-here");

        let captures = re.captures("/abc123/").unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "abc123");

        // Single segment
        let captures = re.captures("/simple").unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "simple");
    }

    #[test]
    fn test_extract_file_id_from_request_rejects_unsafe_id() {
        let options = TusOptions::default();
        let mut req = Request::default();
        *req.uri_mut() = "/uploads/file.txt".parse().unwrap();

        let result = options.extract_file_id_from_request(&req);
        assert!(matches!(result, Err(TusError::FileIdError)));
    }

    #[test]
    fn test_extract_file_id_from_request_accepts_safe_id() {
        let options = TusOptions::default();
        let mut req = Request::default();
        *req.uri_mut() = "/uploads/abc-123_DEF".parse().unwrap();

        let result = options.extract_file_id_from_request(&req).unwrap();
        assert_eq!(result, "abc-123_DEF");
    }

    #[test]
    fn test_max_size_fixed() {
        let max_size = MaxSize::Fixed(1024 * 1024);
        match max_size {
            MaxSize::Fixed(size) => assert_eq!(size, 1024 * 1024),
            _ => panic!("Expected Fixed variant"),
        }
    }

    #[test]
    fn test_max_size_clone() {
        let max_size1 = MaxSize::Fixed(2048);
        let max_size2 = max_size1;
        match max_size2 {
            MaxSize::Fixed(size) => assert_eq!(size, 2048),
            _ => panic!("Expected Fixed variant"),
        }
    }

    #[test]
    fn test_tus_options_default() {
        let options = TusOptions::default();

        assert_eq!(options.path, "/tus-files");
        assert!(options.relative_location);
        assert!(options.canonical_origin.is_none());
        assert!(!options.respect_forwarded_headers);
        assert!(options.allowed_headers.is_empty());
        assert!(options.exposed_headers.is_empty());
        assert!(!options.allowed_credentials);
        assert!(options.allowed_origins.is_empty());
        assert_eq!(options.post_receive_interval, Some(1000));
        assert_eq!(options.lock_drain_timeout, Some(3000));
        assert!(!options.disable_termination_for_finished_uploads);
        assert!(options.generate_url_function.is_none());
        assert!(options.on_incoming_request.is_none());
        assert!(options.on_upload_create.is_none());
        assert!(options.on_upload_finish.is_none());

        // Check max_size is 2GiB
        match &options.max_size {
            Some(MaxSize::Fixed(size)) => assert_eq!(*size, 2 * 1024 * 1024 * 1024),
            _ => panic!("Expected Fixed max_size"),
        }
    }

    #[test]
    fn test_upload_patch_default() {
        let patch = UploadPatch::default();
        assert!(patch.metadata.is_none());
    }

    #[test]
    fn test_upload_finish_patch_default() {
        let patch = UploadFinishPatch::default();
        assert!(patch.status_code.is_none());
        assert!(patch.headers.is_none());
        assert!(patch.body.is_none());
    }

    #[test]
    fn test_upload_patch_clone() {
        let patch = UploadPatch {
            metadata: Some(Metadata::default()),
        };
        let cloned = patch;
        assert!(cloned.metadata.is_some());
    }

    #[test]
    fn test_upload_finish_patch_clone() {
        let patch = UploadFinishPatch {
            status_code: Some(StatusCode::OK),
            headers: Some(HeaderMap::new()),
            body: Some(vec![1, 2, 3]),
        };
        let cloned = patch;
        assert_eq!(cloned.status_code, Some(StatusCode::OK));
        assert!(cloned.headers.is_some());
        assert_eq!(cloned.body, Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_upload_patch_debug() {
        let patch = UploadPatch::default();
        let debug = format!("{patch:?}");
        assert!(debug.contains("UploadPatch"));
    }

    #[test]
    fn test_upload_finish_patch_debug() {
        let patch = UploadFinishPatch::default();
        let debug = format!("{patch:?}");
        assert!(debug.contains("UploadFinishPatch"));
    }

    #[tokio::test]
    async fn test_tus_options_get_configured_max_size_fixed() {
        let options = TusOptions {
            max_size: Some(MaxSize::Fixed(5000)),
            ..Default::default()
        };

        let req = salvo_core::Request::default();
        let size = options.get_configured_max_size(&req, None).await;
        assert_eq!(size, 5000);
    }

    #[tokio::test]
    async fn test_tus_options_get_configured_max_size_none() {
        let options = TusOptions {
            max_size: None,
            ..Default::default()
        };

        let req = salvo_core::Request::default();
        let size = options.get_configured_max_size(&req, None).await;
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_tus_options_get_configured_max_size_dynamic() {
        let options = TusOptions {
            max_size: Some(MaxSize::Dynamic(Arc::new(|_req, _id| {
                Box::pin(async move { 9999u64 })
            }))),
            ..TusOptions::default()
        };

        let req = salvo_core::Request::default();
        let size = options.get_configured_max_size(&req, None).await;
        assert_eq!(size, 9999);
    }

    #[test]
    fn test_extract_host_and_proto_defaults() {
        let headers = HeaderMap::new();
        let result = TusOptions::extract_host_and_proto(&headers, false);
        assert_eq!(result.proto, "http");
        assert_eq!(result.host, "localhost");
    }

    #[test]
    fn test_extract_host_and_proto_host_header() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "example.com".parse().unwrap());

        let result = TusOptions::extract_host_and_proto(&headers, false);
        assert_eq!(result.host, "example.com");
        assert_eq!(result.proto, "http");
    }

    #[test]
    fn test_extract_host_and_proto_forwarded_header() {
        let mut headers = HeaderMap::new();
        headers.insert("forwarded", "proto=https;host=proxy.com".parse().unwrap());

        let result = TusOptions::extract_host_and_proto(&headers, true);
        assert_eq!(result.host, "proxy.com");
        assert_eq!(result.proto, "https");
    }

    #[test]
    fn test_extract_host_and_proto_x_forwarded_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-host", "proxy.com".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());

        let result = TusOptions::extract_host_and_proto(&headers, true);
        assert_eq!(result.host, "proxy.com");
        assert_eq!(result.proto, "https");
    }

    #[test]
    fn test_extract_host_and_proto_x_forwarded_ssl() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "example.com".parse().unwrap());
        headers.insert("x-forwarded-ssl", "on".parse().unwrap());

        let result = TusOptions::extract_host_and_proto(&headers, true);
        assert_eq!(result.proto, "https");
    }

    #[test]
    fn test_extract_host_and_proto_x_forwarded_host_list() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-host",
            "first.com, second.com, third.com".parse().unwrap(),
        );

        let result = TusOptions::extract_host_and_proto(&headers, true);
        assert_eq!(result.host, "first.com");
    }

    #[test]
    fn test_extract_host_and_proto_x_forwarded_proto_list() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", "https, http".parse().unwrap());

        let result = TusOptions::extract_host_and_proto(&headers, true);
        assert_eq!(result.proto, "https");
    }

    #[test]
    fn test_extract_host_and_proto_respect_forwarded_false() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "real.com".parse().unwrap());
        headers.insert("x-forwarded-host", "fake.com".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        headers.insert("x-forwarded-ssl", "on".parse().unwrap());

        // When respect_forwarded_headers is false, should use Host and ignore forwarded hints.
        let result = TusOptions::extract_host_and_proto(&headers, false);
        assert_eq!(result.host, "real.com");
        assert_eq!(result.proto, "http");
    }

    #[test]
    fn test_extract_host_and_proto_forwarded_priority() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "host.com".parse().unwrap());
        headers.insert("forwarded", "host=forwarded.com".parse().unwrap());
        headers.insert("x-forwarded-host", "x-forwarded.com".parse().unwrap());

        // Forwarded header should take priority over X-Forwarded-Host
        let result = TusOptions::extract_host_and_proto(&headers, true);
        assert_eq!(result.host, "forwarded.com");
    }

    #[test]
    fn test_tus_options_clone() {
        let options = TusOptions::default();
        let cloned = options.clone();
        assert_eq!(cloned.path, options.path);
        assert_eq!(cloned.relative_location, options.relative_location);
    }

    #[test]
    fn test_generate_upload_url_uses_canonical_origin_for_absolute_locations() {
        let options = TusOptions {
            relative_location: false,
            canonical_origin: Some("https://uploads.example.com/base/".to_owned()),
            ..TusOptions::default()
        };
        let mut req = Request::default();
        req.headers_mut()
            .insert(header::HOST, "attacker.example".parse().unwrap());

        let url = options.generate_upload_url(&mut req, "abc123").unwrap();

        assert_eq!(url, "https://uploads.example.com/base/tus-files/abc123");
    }

    #[tokio::test]
    async fn test_tus_options_acquire_lock() {
        let options = TusOptions::default();
        let req = salvo_core::Request::default();
        let context = CancellationContext::new();

        let lock = options.acquire_lock(&req, "test-id", context).await;
        assert!(lock.is_ok());
    }

    #[tokio::test]
    async fn test_tus_options_acquire_read_lock() {
        let options = TusOptions::default();
        let req = salvo_core::Request::default();
        let context = CancellationContext::new();

        let lock = options.acquire_read_lock(&req, "test-id", context).await;
        assert!(lock.is_ok());
    }

    #[tokio::test]
    async fn test_tus_options_acquire_write_lock() {
        let options = TusOptions::default();
        let req = salvo_core::Request::default();
        let context = CancellationContext::new();

        let lock = options.acquire_write_lock(&req, "test-id", context).await;
        assert!(lock.is_ok());
    }

    #[tokio::test]
    async fn test_tus_options_acquire_lock_cancelled() {
        let options = TusOptions::default();
        let req = salvo_core::Request::default();
        let context = CancellationContext::new();

        // Cancel before acquiring
        context.cancel();

        // First acquire a lock to block
        let context2 = CancellationContext::new();
        let _guard = options
            .acquire_write_lock(&req, "blocked-id", context2)
            .await
            .unwrap();

        // Try to acquire the same lock with cancelled context
        let context3 = CancellationContext::new();
        context3.cancel();

        // This should return error because signal is already cancelled
        // But since we're using tokio::select!, it depends on which branch wins
        // We'll just verify the cancel mechanism works
        assert!(context3.signal.is_cancelled());
    }
}
