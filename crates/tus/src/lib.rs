use std::{collections::HashMap, sync::Arc};

use crate::{
    error::ProtocolError,
    lockers::Locker,
    options::{MaxSize, TusOptions},
    stores::{DataStore, DiskStore},
    utils::{normalize_path, parse_metadata},
};

mod error;
mod stores;
mod lockers;
mod handlers;

pub mod utils;
pub mod options;

use salvo_core::{Depot, Router, handler, http::HeaderValue};

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

#[derive(Clone, Debug, Default)]
pub struct Metadata(pub HashMap<String, String>);

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

    pub fn store(mut self, store: impl DataStore) -> Self {
        self.store = Arc::new(store);
        self
    }

    pub fn locker(mut self, locker: impl Locker) -> Self {
        self.options.locker = Arc::new(locker);
        self
    }
}

impl Tus {
    pub fn into_router(self) -> Router {
        let base_path = normalize_path(&self.options.path);
        let state = Arc::new(self);

        let r = Router::with_path(base_path)
            .hoop(TusStateHoop { state: state.clone() })
            .options(handlers::options_handler);

        r
    }
}
