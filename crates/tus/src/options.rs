use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use futures_core::future::BoxFuture;
use regex::Regex;
use salvo_core::Request;
use salvo_core::http::{HeaderMap, StatusCode, header};

use crate::CancellationContext;
use crate::error::{TusError, TusResult};
use crate::handlers::{GenerateUrlCtx, HostProto, Metadata};
use crate::lockers::{LockGuard, Locker, memory_locker};
use crate::stores::UploadInfo;

pub type UploadId = Option<String>;

static RE_FILE_ID: OnceLock<Regex> = OnceLock::new();
pub fn get_file_id_regex() -> &'static Regex {
    RE_FILE_ID.get_or_init(|| Regex::new(r"([^/]+)/?$").expect("Invalid regex pattern"))
}

#[derive(Clone)]
pub enum MaxSize {
    Fixed(u64),
    #[allow(clippy::type_complexity)]
    Dynamic(Arc<dyn Fn(&Request, UploadId) -> BoxFuture<'static, u64> + Send + Sync>),
}

pub type NamingFunction = Arc<
    dyn Fn(
            &Request,
            Option<Metadata>,
        ) -> Pin<Box<dyn Future<Output = Result<String, TusError>> + Send>>
        + Send
        + Sync,
>;
pub type GenerateUrlFunction =
    Arc<dyn Fn(&Request, GenerateUrlCtx) -> Result<String, TusError> + Send + Sync>;

pub type OnIncomingRequest =
    Arc<dyn Fn(&Request, String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
pub type OnUploadCreate = Arc<
    dyn Fn(
            &Request,
            UploadInfo,
        ) -> Pin<Box<dyn Future<Output = Result<UploadPatch, TusError>> + Send>>
        + Send
        + Sync,
>;
pub type OnUploadFinish = Arc<
    dyn Fn(
            &Request,
            UploadInfo,
        ) -> Pin<Box<dyn Future<Output = Result<UploadFinishPatch, TusError>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone, Debug, Default)]
pub struct UploadPatch {
    pub metadata: Option<Metadata>,
}

#[derive(Clone, Debug, Default)]
pub struct UploadFinishPatch {
    pub status_code: Option<StatusCode>,
    pub headers: Option<HeaderMap>,
    pub body: Option<Vec<u8>>,
}
#[derive(Clone)]
pub struct TusOptions {
    /// The route to accept requests.
    pub path: String,

    /// Max file size allowed when uploading.
    pub max_size: Option<MaxSize>,

    /// Return a relative URL as the `Location` header.
    pub relative_location: bool,

    /// Allow forwarded headers override Location
    pub respect_forwarded_headers: bool,

    /// Additional headers sent in Access-Control-Allow-Headers
    pub allowed_headers: Vec<String>,

    /// Additional headers sent in Access-Control-Expose-Headers
    pub exposed_headers: Vec<String>,

    /// Set Access-Control-Allow-Credentials
    pub allowed_credentials: bool,

    /// Trusted origins for Access-Control-Allow-Origin
    pub allowed_origins: Vec<String>,

    /// Interval in milliseconds for sending progress
    pub post_receive_interval: Option<u64>,

    /// The Lock interface / provider (required)
    pub locker: Arc<dyn Locker>,

    /// Lock cleanup timeout
    pub lock_drain_timeout: Option<u64>,

    /// Disallow termination for finished uploads
    pub disable_termination_for_finished_uploads: bool,

    /// Function to generate upload IDs
    pub upload_id_naming_function: NamingFunction,

    /// Function to generate file uel
    pub generate_url_function: Option<GenerateUrlFunction>,

    pub on_incoming_request: Option<OnIncomingRequest>,
    pub on_upload_create: Option<OnUploadCreate>,
    pub on_upload_finish: Option<OnUploadFinish>,
}

impl TusOptions {
    pub async fn acquire_lock(
        &self,
        _req: &Request,
        upload_id: &str,
        context: CancellationContext,
    ) -> TusResult<LockGuard> {
        self.acquire_write_lock(_req, upload_id, context).await
    }

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

    pub fn get_file_id_from_request(&self, req: &Request) -> TusResult<String> {
        let path = req.uri().path();
        let re = get_file_id_regex();

        re.captures(path)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or(TusError::FileIdError)
    }

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
            match callback(
                req,
                GenerateUrlCtx {
                    proto,
                    host,
                    path,
                    id: upload_id,
                },
            ) {
                Ok(url) => return Ok(url),
                Err(e) => return Err(e),
            };
        }

        // Default implementation
        if self.relative_location {
            // NOTE: TS version returns `${path}/${id}` — even if path = "" it yields "/id"
            // This matches that behavior.
            return Ok(format!("{path}/{upload_id}"));
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

        // If we still haven't got proto, infer from scheme-ish headers
        // (optional fallback)
        if proto == "http"
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
        TusOptions {
            path: "/tus-files".to_string(),
            max_size: Some(MaxSize::Fixed(2 * 1024 * 1024 * 1024)), // Default max size 2GiB
            relative_location: true,
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
