mod disk;

use std::{collections::{HashMap, HashSet}, pin::Pin};

use bytes::Bytes;
pub use disk::*;

use futures_core::Stream;
use salvo_core::async_trait;

use crate::error::TusResult;

pub type ByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>>;

#[derive(Debug, Clone)]
pub struct StoreInfo {
    pub type_name: String,
    pub path: String,
    pub bucket: Option<String>,
}
#[derive(Debug, Clone)]
pub struct UploadInfo {
    pub id: String,
    pub size: Option<u64>,
    pub offset: u64,
    pub metadata: Option<HashMap<String, String>>,
    pub storage: Option<StoreInfo>,
    pub creation_date: String,
}

impl UploadInfo {
    pub fn new(id: String, offset: u64) -> Self {
        Self {
            id,
            size: None,
            offset,
            metadata: None,
            storage: None,
            creation_date: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn get_size_is_deferred(&self) -> bool {
        self.size.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Extension {
    Expiration,
    CreationDeferLength,
    Concatentation,
}
#[async_trait]
pub trait DataStore: Send + Sync + 'static {
    fn extensions(&self) -> HashSet<Extension> {
        HashSet::new()
    }

    fn has_extension(&self, ext: Extension) -> bool {
        self.extensions().contains(&ext)
    }

    async fn create(&self, file: UploadInfo) -> TusResult<UploadInfo>;
    async fn remove(&self, id: &str) -> TusResult<()>;
    async fn write(&self, id: &str, offset: u64, stream: ByteStream) -> TusResult<u64>;
    async fn get_upload(&self, id: &str) -> TusResult<UploadInfo>;
    async fn declare_upload_length(&self, id: &str, length: u64) -> TusResult<()>;

    async fn delete_expired(&self) -> TusResult<u32> {
        Ok(0)
    }
    fn get_expiration(&self) -> Option<std::time::Duration> {
        None
    }
}