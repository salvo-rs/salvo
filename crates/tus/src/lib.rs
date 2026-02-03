//! TUS (Resumable Upload Protocol) implementation for the Salvo web framework.
//!
//! [TUS](https://tus.io/) is an open protocol for resumable file uploads over HTTP.
//! It allows reliable uploads of large files by enabling pause and resume functionality,
//! making it ideal for unreliable network conditions.
//!
//! # Features
//!
//! - Resumable uploads - Clients can resume interrupted uploads
//! - Upload metadata - Attach custom metadata to uploads
//! - Configurable max size - Limit upload file sizes
//! - Lifecycle hooks - React to upload events
//! - Customizable storage - Implement your own storage backend
//!
//! # Example
//!
//! ```ignore
//! use salvo_tus::{Tus, MaxSize};
//! use salvo_core::prelude::*;
//!
//! let tus = Tus::new()
//!     .path("/uploads")
//!     .max_size(MaxSize::Fixed(100 * 1024 * 1024));  // 100 MB limit
//!
//! let router = Router::new()
//!     .push(tus.into_router());
//!
//! let acceptor = TcpListener::new("0.0.0.0:8080").bind().await;
//! Server::new(acceptor).serve(router).await;
//! ```
//!
//! # TUS Protocol Endpoints
//!
//! The router created by `into_router()` handles:
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | OPTIONS | `/uploads` | Returns TUS protocol capabilities |
//! | POST | `/uploads` | Creates a new upload |
//! | HEAD | `/uploads/{id}` | Returns upload progress |
//! | PATCH | `/uploads/{id}` | Uploads a chunk |
//! | DELETE | `/uploads/{id}` | Cancels an upload |
//! | GET | `/uploads/{id}` | Downloads the uploaded file |
//!
//! # Lifecycle Hooks
//!
//! React to upload events:
//!
//! ```ignore
//! let tus = Tus::new()
//!     .with_on_upload_create(|req, upload_info| async move {
//!         println!("New upload: {:?}", upload_info);
//!         Ok(UploadPatch::default())
//!     })
//!     .with_on_upload_finish(|req, upload_info| async move {
//!         println!("Upload complete: {:?}", upload_info);
//!         Ok(UploadFinishPatch::default())
//!     });
//! ```
//!
//! # Custom Upload ID
//!
//! Generate custom upload IDs:
//!
//! ```ignore
//! let tus = Tus::new()
//!     .with_upload_id_naming_function(|req, metadata| async move {
//!         Ok(uuid::Uuid::new_v4().to_string())
//!     });
//! ```
//!
//! # Storage Backends
//!
//! By default, files are stored on disk using `DiskStore`.
//! Implement `DataStore` trait for custom storage (S3, database, etc.).
//!
//! Read more: <https://salvo.rs>

use std::sync::Arc;

use tokio::sync::watch;

use crate::error::TusError;
use crate::handlers::{GenerateUrlCtx, Metadata};
use crate::lockers::Locker;
use crate::options::{MaxSize, TusOptions, UploadFinishPatch, UploadPatch};
use crate::stores::{DataStore, DiskStore, UploadInfo};
use crate::utils::normalize_path;

mod error;
mod handlers;
mod lockers;
mod stores;

pub mod options;
pub mod utils;

use salvo_core::{Depot, Request, Router, handler};

pub const TUS_VERSION: &str = "1.0.0";
pub const H_TUS_RESUMABLE: &str = "tus-resumable";
pub const H_TUS_VERSION: &str = "tus-version";
pub const H_TUS_EXTENSION: &str = "tus-extension";
pub const H_TUS_MAX_SIZE: &str = "tus-max-size";

pub const H_ACCESS_CONTROL_ALLOW_METHODS: &str = "access-control-allow-methods";
pub const H_ACCESS_CONTROL_ALLOW_HEADERS: &str = "access-control-allow-headers";
pub const H_ACCESS_CONTROL_REQUEST_HEADERS: &str = "access-control-request-headers";
pub const H_ACCESS_CONTROL_MAX_AGE: &str = "access-control-max-age";

pub const H_UPLOAD_LENGTH: &str = "upload-length";
pub const H_UPLOAD_OFFSET: &str = "upload-offset";
pub const H_UPLOAD_METADATA: &str = "upload-metadata";
pub const H_UPLOAD_CONCAT: &str = "upload-concat";
pub const H_UPLOAD_DEFER_LENGTH: &str = "upload-defer-length";
pub const H_UPLOAD_EXPIRES: &str = "upload-expires";

pub const H_CONTENT_TYPE: &str = "content-type";
pub const H_CONTENT_LENGTH: &str = "content-length";
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
impl Default for Tus {
    fn default() -> Self {
        Self::new()
    }
}

// Tus service Configuration
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

    pub fn into_router(self) -> Router {
        let base_path = normalize_path(&self.options.path);
        let state = Arc::new(self);

        Router::with_path(base_path)
            .hoop(TusStateHoop {
                state: state.clone(),
            })
            .push(handlers::options_handler())
            .push(handlers::post_handler())
            .push(handlers::head_handler())
            .push(handlers::patch_handler())
            .push(handlers::delete_handler())
            .push(handlers::get_handler())
    }
}

// Hooks
impl Tus {
    pub fn with_upload_id_naming_function<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(&Request, Option<Metadata>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<String, TusError>> + Send + 'static,
    {
        self.options.upload_id_naming_function = Arc::new(move |req, meta| Box::pin(f(req, meta)));
        self
    }

    pub fn with_generate_url_function<F>(mut self, f: F) -> Self
    where
        F: Fn(&Request, GenerateUrlCtx) -> Result<String, TusError> + Send + Sync + 'static,
    {
        self.options.generate_url_function = Some(Arc::new(f));
        self
    }
}

// Lifecycle
impl Tus {
    pub fn with_on_incoming_request<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(&Request, String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.options.on_incoming_request = Some(Arc::new(move |req, id| Box::pin(f(req, id))));
        self
    }

    pub fn with_on_incoming_request_sync<F>(mut self, f: F) -> Self
    where
        F: Fn(&Request, String) + Send + Sync + 'static,
    {
        self.options.on_incoming_request = Some(Arc::new(move |req, id| {
            f(req, id);
            Box::pin(async move {})
        }));
        self
    }

    pub fn with_on_upload_create<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(&Request, UploadInfo) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<UploadPatch, TusError>> + Send + 'static,
    {
        self.options.on_upload_create = Some(Arc::new(move |req, upload| Box::pin(f(req, upload))));
        self
    }

    pub fn with_on_upload_finish<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(&Request, UploadInfo) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<UploadFinishPatch, TusError>> + Send + 'static,
    {
        self.options.on_upload_finish = Some(Arc::new(move |req, upload| Box::pin(f(req, upload))));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(TUS_VERSION, "1.0.0");
        assert_eq!(H_TUS_RESUMABLE, "tus-resumable");
        assert_eq!(H_TUS_VERSION, "tus-version");
        assert_eq!(H_TUS_EXTENSION, "tus-extension");
        assert_eq!(H_TUS_MAX_SIZE, "tus-max-size");
        assert_eq!(H_UPLOAD_LENGTH, "upload-length");
        assert_eq!(H_UPLOAD_OFFSET, "upload-offset");
        assert_eq!(H_UPLOAD_METADATA, "upload-metadata");
        assert_eq!(H_UPLOAD_CONCAT, "upload-concat");
        assert_eq!(H_UPLOAD_DEFER_LENGTH, "upload-defer-length");
        assert_eq!(H_UPLOAD_EXPIRES, "upload-expires");
        assert_eq!(H_CONTENT_TYPE, "content-type");
        assert_eq!(H_CONTENT_LENGTH, "content-length");
        assert_eq!(CT_OFFSET_OCTET_STREAM, "application/offset+octet-stream");
    }

    #[test]
    fn test_cancellation_reason_equality() {
        assert_eq!(CancellationReason::Abort, CancellationReason::Abort);
        assert_eq!(CancellationReason::Cancel, CancellationReason::Cancel);
        assert_ne!(CancellationReason::Abort, CancellationReason::Cancel);
    }

    #[test]
    fn test_cancellation_reason_clone_copy() {
        let reason = CancellationReason::Abort;
        let cloned = reason.clone();
        let copied = reason;
        assert_eq!(reason, cloned);
        assert_eq!(reason, copied);
    }

    #[test]
    fn test_cancellation_reason_debug() {
        let debug = format!("{:?}", CancellationReason::Abort);
        assert_eq!(debug, "Abort");

        let debug = format!("{:?}", CancellationReason::Cancel);
        assert_eq!(debug, "Cancel");
    }

    #[test]
    fn test_cancellation_context_new() {
        let ctx = CancellationContext::new();
        assert!(!ctx.signal.is_cancelled());
        assert!(!ctx.signal.is_aborted());
        assert!(ctx.signal.reason().is_none());
    }

    #[test]
    fn test_cancellation_context_default() {
        let ctx = CancellationContext::default();
        assert!(!ctx.signal.is_cancelled());
    }

    #[test]
    fn test_cancellation_context_abort() {
        let ctx = CancellationContext::new();
        ctx.abort();

        assert!(ctx.signal.is_cancelled());
        assert!(ctx.signal.is_aborted());
        assert_eq!(ctx.signal.reason(), Some(CancellationReason::Abort));
    }

    #[test]
    fn test_cancellation_context_cancel() {
        let ctx = CancellationContext::new();
        ctx.cancel();

        assert!(ctx.signal.is_cancelled());
        assert!(!ctx.signal.is_aborted());
        assert_eq!(ctx.signal.reason(), Some(CancellationReason::Cancel));
    }

    #[test]
    fn test_cancellation_signal_clone() {
        let ctx = CancellationContext::new();
        let signal1 = ctx.signal.clone();
        let signal2 = ctx.signal.clone();

        assert!(!signal1.is_cancelled());
        assert!(!signal2.is_cancelled());

        ctx.abort();

        assert!(signal1.is_cancelled());
        assert!(signal2.is_cancelled());
    }

    #[test]
    fn test_cancellation_context_clone() {
        let ctx1 = CancellationContext::new();
        let ctx2 = ctx1.clone();

        // Both contexts share the same sender/receiver
        ctx1.abort();

        assert!(ctx2.signal.is_cancelled());
    }

    #[tokio::test]
    async fn test_cancellation_signal_cancelled_async() {
        let ctx = CancellationContext::new();
        let mut signal = ctx.signal.clone();

        // Spawn a task to cancel after a short delay
        let ctx_clone = ctx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            ctx_clone.cancel();
        });

        let reason = signal.cancelled().await;
        assert_eq!(reason, CancellationReason::Cancel);
    }

    #[tokio::test]
    async fn test_cancellation_signal_cancelled_already_cancelled() {
        let ctx = CancellationContext::new();
        ctx.abort();

        let mut signal = ctx.signal.clone();
        let reason = signal.cancelled().await;
        assert_eq!(reason, CancellationReason::Abort);
    }

    #[test]
    fn test_cancellation_context_debug() {
        let ctx = CancellationContext::new();
        let debug = format!("{:?}", ctx);
        assert!(debug.contains("CancellationContext"));
    }

    #[test]
    fn test_cancellation_signal_debug() {
        let ctx = CancellationContext::new();
        let debug = format!("{:?}", ctx.signal);
        assert!(debug.contains("CancellationSignal"));
    }

    #[test]
    fn test_tus_new() {
        let tus = Tus::new();
        assert_eq!(tus.options.path, "/tus-files");
    }

    #[test]
    fn test_tus_default() {
        let tus = Tus::default();
        assert_eq!(tus.options.path, "/tus-files");
    }

    #[test]
    fn test_tus_path() {
        let tus = Tus::new().path("/custom/uploads");
        assert_eq!(tus.options.path, "/custom/uploads");
    }

    #[test]
    fn test_tus_max_size() {
        let tus = Tus::new().max_size(MaxSize::Fixed(1024 * 1024));
        match &tus.options.max_size {
            Some(MaxSize::Fixed(size)) => assert_eq!(*size, 1024 * 1024),
            _ => panic!("Expected Fixed max_size"),
        }
    }

    #[test]
    fn test_tus_relative_location() {
        let tus = Tus::new().relative_location(false);
        assert!(!tus.options.relative_location);

        let tus = Tus::new().relative_location(true);
        assert!(tus.options.relative_location);
    }

    #[test]
    fn test_tus_with_locker() {
        use lockers::memory_locker::MemoryLocker;

        let tus = Tus::new().with_locker(MemoryLocker::new());
        // Just verify it compiles and doesn't panic
        assert!(Arc::strong_count(&tus.options.locker) >= 1);
    }

    #[test]
    fn test_tus_with_store() {
        let tus = Tus::new().with_store(stores::DiskStore::new());
        // Just verify it compiles and doesn't panic
        assert!(Arc::strong_count(&tus.store) >= 1);
    }

    #[test]
    fn test_tus_into_router() {
        let tus = Tus::new().path("/uploads");
        let _router = tus.into_router();
        // Router creation should succeed
    }

    #[test]
    fn test_tus_clone() {
        let tus = Tus::new().path("/test");
        let cloned = tus.clone();
        assert_eq!(cloned.options.path, "/test");
    }

    #[test]
    fn test_tus_builder_chain() {
        let tus = Tus::new()
            .path("/api/tus")
            .max_size(MaxSize::Fixed(10 * 1024 * 1024))
            .relative_location(false);

        assert_eq!(tus.options.path, "/api/tus");
        assert!(!tus.options.relative_location);
        match &tus.options.max_size {
            Some(MaxSize::Fixed(size)) => assert_eq!(*size, 10 * 1024 * 1024),
            _ => panic!("Expected Fixed max_size"),
        }
    }

    #[tokio::test]
    async fn test_tus_with_upload_id_naming_function() {
        let tus = Tus::new().with_upload_id_naming_function(|_req, _meta| async move {
            Ok("custom-id".to_string())
        });

        // Verify the function is set by calling it
        let req = Request::default();
        let result = (tus.options.upload_id_naming_function)(&req, None).await;
        assert_eq!(result.unwrap(), "custom-id");
    }

    #[test]
    fn test_tus_with_generate_url_function() {
        let tus = Tus::new().with_generate_url_function(|_req, ctx| {
            Ok(format!("https://cdn.example.com/{}", ctx.id))
        });

        assert!(tus.options.generate_url_function.is_some());
    }

    #[tokio::test]
    async fn test_tus_with_on_incoming_request() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let tus = Tus::new().with_on_incoming_request(move |_req, _id| {
            let called = called_clone.clone();
            async move {
                called.store(true, Ordering::SeqCst);
            }
        });

        assert!(tus.options.on_incoming_request.is_some());
        let req = Request::default();
        (tus.options.on_incoming_request.unwrap())(&req, "test-id".to_string()).await;
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_tus_with_on_incoming_request_sync() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let tus = Tus::new().with_on_incoming_request_sync(move |_req, _id| {
            called_clone.store(true, Ordering::SeqCst);
        });

        assert!(tus.options.on_incoming_request.is_some());
    }

    #[tokio::test]
    async fn test_tus_with_on_upload_create() {
        let tus = Tus::new()
            .with_on_upload_create(|_req, _upload| async move { Ok(UploadPatch::default()) });

        assert!(tus.options.on_upload_create.is_some());
    }

    #[tokio::test]
    async fn test_tus_with_on_upload_finish() {
        let tus = Tus::new()
            .with_on_upload_finish(|_req, _upload| async move { Ok(UploadFinishPatch::default()) });

        assert!(tus.options.on_upload_finish.is_some());
    }
}
