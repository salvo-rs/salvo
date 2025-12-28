use std::sync::Arc;

use futures_core::future::BoxFuture;
use salvo_core::Request;

use crate::{
    CancellationContext, error::{TusError, TusResult}, handlers::GenerateUrlCtx, lockers::{LockGuard, Locker, memory_locker}
};

pub type UploadId = Option<String>;

#[derive(Clone)]
pub enum MaxSize {
    Fixed(u64),
    Dynamic(Arc<dyn Fn(&Request, UploadId) -> BoxFuture<'static, u64> + Send + Sync>),
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
    pub upload_id_naming_function: Arc<dyn Fn(&Request, crate::Metadata) -> Result<String, TusError> + Send + Sync>,

    pub generate_url_function: Option<Arc<dyn Fn(&Request, GenerateUrlCtx)-> Result<String, TusError> + Send + Sync>>,
}

impl TusOptions {
    pub async fn get_configured_max_size(&self, req: &Request, upload_id: &str) -> u64 {
        match &self.max_size {
            Some(MaxSize::Fixed(size)) => *size,
            Some(MaxSize::Dynamic(func)) => {
                let fut = func(req, Some(upload_id.to_string()));
                fut.await
            }
            None => 0,
        }
    }

    pub async fn acquire_lock(
        &self,
        _req: &Request,
        upload_id: &str,
        context: CancellationContext,
    ) -> TusResult<LockGuard> {
        let mut signal = context.signal.clone();
        tokio::select! {
            lock = self.locker.lock(upload_id) => lock,
            reason = signal.cancelled() => Err(TusError::Internal(format!("request {reason:?}"))),
        }
    }
}

impl Default for TusOptions {
    fn default() -> Self {
        TusOptions {
            path: "/tus-files".to_string(),
            max_size: None,
            relative_location: true,
            respect_forwarded_headers: false,
            allowed_headers: vec![],
            exposed_headers: vec![],
            allowed_credentials: false,
            allowed_origins: vec![],
            post_receive_interval: Some(1000),
            locker: Arc::new(memory_locker::MemoryLocker::new()),
            lock_drain_timeout: Some(3000),
            disable_termination_for_finished_uploads: false,
            // Default use uuid.
            upload_id_naming_function: Arc::new(|_req, _metadata| {
                    use uuid::Uuid;
                    Ok(Uuid::new_v4().to_string())
            }),
            generate_url_function: None,
            // on_incoming_request: None,
            // on_upload_create: None,
            // on_upload_finish: None,
        }
    }
}
