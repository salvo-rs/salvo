use std::sync::Arc;

use tokio::sync::watch;

use crate::{
    error::TusError, handlers::{GenerateUrlCtx, HostProto, Metadata}, lockers::Locker, options::{MaxSize, TusOptions}, stores::{DataStore, DiskStore}, utils::normalize_path
};

mod error;
mod stores;
mod lockers;
mod handlers;

pub mod utils;
pub mod options;

use salvo_core::{Depot, Request, Router, handler, http::{HeaderMap, header}};

pub const TUS_VERSION: &str = "1.0.0";
pub const H_TUS_RESUMABLE: &str = "tus-resumable";
pub const H_TUS_VERSION: &str = "tus-version";
pub const H_TUS_EXTENSION: &str = "tus-extension";
pub const H_TUS_MAX_SIZE: &str = "tus-max-size";

pub const H_UPLOAD_LENGTH: &str = "upload-length";
pub const H_UPLOAD_OFFSET: &str = "upload-offset";
pub const H_UPLOAD_METADATA: &str = "upload-metadata";

pub const H_CONTENT_TYPE: &str = "content-type";
pub const CT_OFFSET_OCTET_STREAM: &str = "application/offset+octet-stream";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationReason {
    Abort,
    Cancel,
}

#[derive(Clone, Debug)]
pub struct CancellationSignal {
    receiver: watch::Receiver<Option<CancellationReason>>,
}

impl CancellationSignal {
    pub fn reason(&self) -> Option<CancellationReason> {
        *self.receiver.borrow()
    }

    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }

    pub fn is_aborted(&self) -> bool {
        matches!(self.reason(), Some(CancellationReason::Abort))
    }

    pub async fn cancelled(&mut self) -> CancellationReason {
        loop {
            if let Some(reason) = *self.receiver.borrow() {
                return reason;
            }
            if self.receiver.changed().await.is_err() {
                return CancellationReason::Cancel;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct CancellationContext {
    pub signal: CancellationSignal,
    sender: watch::Sender<Option<CancellationReason>>,
}

impl CancellationContext {
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(None);
        Self {
            signal: CancellationSignal { receiver },
            sender,
        }
    }

    pub fn abort(&self) {
        let _ = self.sender.send(Some(CancellationReason::Abort));
    }

    pub fn cancel(&self) {
        let _ = self.sender.send(Some(CancellationReason::Cancel));
    }
}

impl Default for CancellationContext {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct TusStateHoop {
    state: Arc<Tus>,
}

#[handler]
impl TusStateHoop {
    async fn handle(&self, depot: &mut Depot) {
        depot.inject(self.state.clone());
    }
}

#[derive(Clone)]
pub struct Tus {
    options: TusOptions,
    store: Arc<dyn DataStore>,
}

impl Tus {
    pub fn new() -> Self {
        Self {
            options: TusOptions::default(),
            store: Arc::new(DiskStore::new()),
        }
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.options.path = path.into();
        self
    }

    pub fn max_size(mut self, max_size: MaxSize) -> Self {
        self.options.max_size = Some(max_size);
        self
    }

    pub fn relative_location(mut self, yes: bool) -> Self {
        self.options.relative_location = yes;
        self
    }

    pub fn with_store(mut self, store: impl DataStore) -> Self {
        self.store = Arc::new(store);
        self
    }

    pub fn with_locker(mut self, locker: impl Locker) -> Self {
        self.options.locker = Arc::new(locker);
        self
    }

    pub fn with_upload_id_naming_function<F>(mut self, f: F) -> Self
    where
        F: Fn(&Request, Metadata) -> Result<String, crate::error::TusError> + Send + Sync + 'static,
    {
        self.options.upload_id_naming_function = Arc::new(f);
        self
    }

    pub fn with_generate_url_function<F>(mut self, f: F) -> Self
    where
        F: Fn(&Request, GenerateUrlCtx) -> Result<String, crate::error::TusError> + Send + Sync + 'static,
    {
        self.options.generate_url_function = Some(Arc::new(f));
        self
    }
}

impl Tus {
    pub fn into_router(self) -> Router {
        let base_path = normalize_path(&self.options.path);
        let state = Arc::new(self);

        let router = Router::with_path(base_path)
            .hoop(TusStateHoop { state: state.clone() })
            .push(handlers::options_handler())
            .push(handlers::post_handler())
            .push(handlers::head_handler())
            .push(handlers::patch_handler());

        router
    }

    pub fn generate_upload_url(&self, req: &mut Request, upload_id: &str) -> Result<String, TusError> {
        let path = if self.options.path == "/" {
            ""
        } else {
            self.options.path.as_str()
        };

         let HostProto { proto, host } =
                Self::extract_host_and_proto(req.headers(), self.options.respect_forwarded_headers);

        if let Some(callback) = &self.options.generate_url_function {
            match callback(&req, GenerateUrlCtx {
                proto,
                host,
                path,
                id: upload_id,
            }) {
                Ok(url) => return Ok(url),
                Err(e) => return Err(e),
            };
        }


        // Default implementation
        if self.options.relative_location {
            // NOTE: TS version returns `${path}/${id}` — even if path = "" it yields "/id"
            // This matches that behavior.
            return Ok(format!("{}/{}", path, upload_id));
        }

        Ok(format!("{}://{}{}{}", proto, host, path, format!("/{}", upload_id)))
    }

    /// Rust version of BaseHandler.extractHostAndProto(...)
    pub fn extract_host_and_proto<'a>(
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
            if host == "localhost" {
                if let Some(v) = headers
                    .get("x-forwarded-host")
                    .and_then(|v| v.to_str().ok())
                {
                    // x-forwarded-host may contain comma-separated list; use the first one
                    host = v.split(',').next().unwrap_or(v).trim();
                }
            }

            // 2) determine proto (X-Forwarded-Proto)
            if proto == "http" {
                if let Some(v) = headers
                    .get("x-forwarded-proto")
                    .and_then(|v| v.to_str().ok())
                {
                    proto = v.split(',').next().unwrap_or(v).trim();
                }
            }
        }

        // If we still haven't got a host, use Host header
        if host == "localhost" {
            if let Some(v) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
                host = v.trim();
            }
        }

        // If we still haven't got proto, infer from scheme-ish headers
        // (optional fallback)
        if proto == "http" {
            if let Some(v) = headers.get("x-forwarded-ssl").and_then(|v| v.to_str().ok()) {
                if v.eq_ignore_ascii_case("on") {
                    proto = "https";
                }
            }
        }

        HostProto { proto, host }
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
