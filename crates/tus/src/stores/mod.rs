mod disk;

use std::{collections::HashMap, pin::Pin};

use bytes::Bytes;
pub use disk::*;

use futures_core::Stream;
use salvo_core::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::TusResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetaFile {
    id: String,
    length: u64,
    offset: u64,
    metadata: HashMap<String, String>,
}

impl From<UploadInfo> for MetaFile {
    fn from(i: UploadInfo) -> Self {
        Self {
            id: i.id,
            length: i.length,
            offset: i.offset,
            metadata: i.metadata,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UploadInfo {
    pub id: String,
    pub length: u64,
    pub offset: u64,
    pub metadata: HashMap<String, String>,
}

impl From<MetaFile> for UploadInfo {
    fn from(m: MetaFile) -> Self {
        Self {
            id: m.id,
            length: m.length,
            offset: m.offset,
            metadata: m.metadata,
        }
    }
}

#[derive(Debug)]
pub struct NewUpload {
    pub length: u64,
    pub metadata: HashMap<String, String>,
}

pub type ByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>>;

#[async_trait]
pub trait DataStore: Send + Sync + 'static {
    async fn create(&self, new: NewUpload) -> TusResult<UploadInfo>;
    async fn get(&self, id: &str) -> TusResult<UploadInfo>;
    async fn set_offset(&self, id: &str, offset: u64) -> TusResult<()>;
    async fn write(&self, id: &str, offset: u64, stream: ByteStream) -> TusResult<u64>;
}