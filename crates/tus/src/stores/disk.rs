use std::path::PathBuf;

use salvo_core::async_trait;

use crate::{error::TusResult, stores::{ByteStream, DataStore, UploadInfo}};

#[derive(Clone)]
pub struct DiskStore {
    root: PathBuf,
}

impl DiskStore {
    pub fn new() -> Self {
        Self {
            root: "./tus-data".into(),
        }
    }

    pub fn disk_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = root.into();
        self
    }
}

impl DiskStore {}

#[async_trait]
impl DataStore for DiskStore {
    async fn create(&self, file: UploadInfo) -> TusResult<UploadInfo> {
        todo!()
    }
    async fn remove(&self, id: &str) -> TusResult<()> {
        todo!()
    }
    async fn write(&self, id: &str, offset: u64, stream: ByteStream) -> TusResult<u64> {
        todo!()
    }
    async fn get_upload(&self, id: &str) -> TusResult<UploadInfo> {
        todo!()
    }
    async fn declare_upload_length(&self, id: &str, length: u64) -> TusResult<()> {
        todo!()
    }
}