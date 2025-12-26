use std::sync::Arc;

use crate::{error::TusResult, lockers::{Locker, MemoryLocker}, stores::{DataStore, DiskStore}};

mod error;
mod stores;
mod lockers;
mod handlers;

mod utils;

use salvo_core::Router;
pub use utils::*;

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

#[derive(Clone)]
pub struct Tus {
    base_path: String,
    max_size: Option<u64>,
    relative_location: bool,

    store: Arc<dyn DataStore>,
    locker: Arc<dyn Locker>,
}

impl Tus {
    pub fn new() -> Self {
        Self {
            base_path: "/tus-files".to_string(),
            max_size: None,
            relative_location: true,
            store: Arc::new(DiskStore::new()),
            locker: Arc::new(MemoryLocker::new()),
        }
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.base_path = path.into();
        self
    }

    pub fn max_size(mut self, bytes: u64) -> Self {
        self.max_size = Some(bytes);
        self
    }

    pub fn relative_location(mut self, yes: bool) -> Self {
        self.relative_location = yes;
        self
    }

    pub fn store(mut self, store: impl DataStore) -> Self {
        self.store = Arc::new(store);
        self
    }

    pub fn locker(mut self, locker: impl Locker) -> Self {
        self.locker = Arc::new(locker);
        self
    }
}

impl Tus {
    pub fn into_router(self) -> Router {
        let base_path = normalize_path(&self.base_path);

        let r = Router::with_path(base_path)
            .options(handlers::options_handler);

        r
    }
}