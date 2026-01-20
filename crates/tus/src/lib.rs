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

        let router = Router::with_path(base_path)
            .hoop(TusStateHoop {
                state: state.clone(),
            })
            .push(handlers::options_handler())
            .push(handlers::post_handler())
            .push(handlers::head_handler())
            .push(handlers::patch_handler())
            .push(handlers::delete_handler())
            .push(handlers::get_handler());

        router
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
