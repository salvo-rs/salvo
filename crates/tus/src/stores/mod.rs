mod disk;

use std::{collections::HashSet, pin::Pin};

use bytes::Bytes;
pub use disk::*;

use futures_core::Stream;
use salvo_core::{async_trait, http::HeaderValue};

use crate::{error::TusResult, handlers::Metadata};

pub type ByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>>;


// #[derive(Debug, Clone)]
// pub enum StoreType {
//     Disk,
// }

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
    pub offset: Option<u64>,
    pub metadata: Option<Metadata>,
    pub storage: Option<StoreInfo>,
    pub creation_date: String,
}

impl UploadInfo {
    pub fn new(id: String) -> Self {
        Self {
            id,
            size: None,
            offset: None,
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
    Creation,
    Expiration,
    CreationDeferLength,
    Concatenation,
}

impl Extension {
    pub fn as_str(&self) -> &'static str {
        match self {
            Extension::Creation => "creation",
            Extension::Expiration => "expiration",
            Extension::CreationDeferLength => "creation-defer-length",
            Extension::Concatenation => "concatenation"
        }
    }

    pub fn to_header_value(extensions: &HashSet<Extension>) -> Option<HeaderValue> {
        if extensions.is_empty() {
            return None;
        }

        let value = extensions
            .iter()
            .map(|e| e.as_str())
            .collect::<Vec<_>>()
            .join(",");

        HeaderValue::from_str(&value).ok()
    }
}

/// Extension:
/// Default extensions is empty.
/// Clients and Servers are encouraged to implement as many of the extensions as possible.
/// Feature detection SHOULD be achieved by the Client sending an OPTIONS request and the Server responding with the Tus-Extension header.
/// See more details: https://tus.io/protocols/resumable-upload#protocol-extensions
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
    async fn get_upload_file_info(&self, id: &str) -> TusResult<UploadInfo>;
    async fn declare_upload_length(&self, id: &str, length: u64) -> TusResult<()>;

    async fn delete_expired(&self) -> TusResult<u32> {
        Ok(0)
    }
    fn get_expiration(&self) -> Option<std::time::Duration> {
        None
    }
}