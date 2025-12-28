use std::{collections::HashMap, path::PathBuf};

use salvo_core::async_trait;
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    io::{self, AsyncSeekExt, AsyncWriteExt},
};

use crate::{
    error::{TusError, TusResult},
    handlers::Metadata,
    stores::{ByteStream, DataStore, StoreInfo, UploadInfo},
};

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

    async fn ensure_root(&self) -> TusResult<()> {
        fs::create_dir_all(&self.root)
            .await
            .map_err(|e| TusError::Internal(e.to_string()))
    }

    fn data_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.bin"))
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }

    fn meta_tmp_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json.tmp"))
    }

    async fn read_meta(&self, id: &str) -> TusResult<MetaUpload> {
        let path = self.meta_path(id);
        let bytes = fs::read(path).await.map_err(map_io_error)?;
        serde_json::from_slice::<MetaUpload>(&bytes)
            .map_err(|e| TusError::Internal(format!("invalid meta json: {e}")))
    }

    async fn write_meta_atomic(&self, meta: &MetaUpload) -> TusResult<()> {
        let id = &meta.id;
        let tmp = self.meta_tmp_path(id);
        let final_path = self.meta_path(id);

        let json = serde_json::to_vec(meta)
            .map_err(|e| TusError::Internal(format!("serialize meta json: {e}")))?;

        fs::write(&tmp, json)
            .await
            .map_err(|e| TusError::Internal(e.to_string()))?;

        #[cfg(windows)]
        {
            let _ = fs::remove_file(&final_path).await;
        }

        fs::rename(&tmp, &final_path)
            .await
            .map_err(|e| TusError::Internal(format!("rename meta tmp: {e}")))?;

        Ok(())
    }
}

fn map_io_error(e: io::Error) -> TusError {
    match e.kind() {
        io::ErrorKind::NotFound => TusError::NotFound,
        _ => TusError::Internal(e.to_string()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetaStoreInfo {
    type_name: String,
    path: String,
    bucket: Option<String>,
}

impl From<StoreInfo> for MetaStoreInfo {
    fn from(info: StoreInfo) -> Self {
        Self {
            type_name: info.type_name,
            path: info.path,
            bucket: info.bucket,
        }
    }
}

impl From<MetaStoreInfo> for StoreInfo {
    fn from(info: MetaStoreInfo) -> Self {
        Self {
            type_name: info.type_name,
            path: info.path,
            bucket: info.bucket,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetaUpload {
    id: String,
    size: Option<u64>,
    offset: u64,
    metadata: Option<HashMap<String, String>>,
    storage: Option<MetaStoreInfo>,
    creation_date: String,
}

impl From<UploadInfo> for MetaUpload {
    fn from(info: UploadInfo) -> Self {
        Self {
            id: info.id,
            size: info.size,
            offset: info.offset.unwrap_or(0),
            metadata: info.metadata.map(|m| m.0),
            storage: info.storage.map(MetaStoreInfo::from),
            creation_date: info.creation_date,
        }
    }
}

impl From<MetaUpload> for UploadInfo {
    fn from(info: MetaUpload) -> Self {
        Self {
            id: info.id,
            size: info.size,
            offset: Some(info.offset),
            metadata: info.metadata.map(Metadata),
            storage: info.storage.map(StoreInfo::from),
            creation_date: info.creation_date,
        }
    }
}

#[async_trait]
impl DataStore for DiskStore {
    async fn create(&self, file: UploadInfo) -> TusResult<UploadInfo> {
        self.ensure_root().await?;

        let mut file = file;
        if file.storage.is_none() {
            file.storage = Some(StoreInfo {
                type_name: "file".to_string(),
                path: self.data_path(&file.id).to_string_lossy().to_string(),
                bucket: None,
            });
        }

        let data_path = self.data_path(&file.id);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&data_path)
            .await
            .map_err(|e| TusError::Internal(e.to_string()))?;

        let meta = MetaUpload::from(file.clone());
        if let Err(err) = self.write_meta_atomic(&meta).await {
            let _ = fs::remove_file(&data_path).await;
            return Err(err);
        }

        Ok(file)
    }

    async fn remove(&self, id: &str) -> TusResult<()> {
        self.ensure_root().await?;

        let data_path = self.data_path(id);
        let meta_path = self.meta_path(id);
        let mut removed = false;

        match fs::remove_file(&data_path).await {
            Ok(_) => removed = true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(TusError::Internal(e.to_string())),
        }

        match fs::remove_file(&meta_path).await {
            Ok(_) => removed = true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(TusError::Internal(e.to_string())),
        }

        if removed {
            Ok(())
        } else {
            Err(TusError::NotFound)
        }
    }

    async fn write(&self, id: &str, offset: u64, stream: ByteStream) -> TusResult<u64> {
        use std::io::SeekFrom;
        use futures_util::StreamExt;

        self.ensure_root().await?;

        let mut meta = self.read_meta(id).await?;
        if meta.offset != offset {
            return Err(TusError::OffsetMismatch {
                expected: meta.offset,
                got: offset,
            });
        }

        let path = self.data_path(id);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .await
            .map_err(map_io_error)?;

        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| TusError::Internal(e.to_string()))?;

        let mut written: u64 = 0;
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| TusError::Internal(e.to_string()))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| TusError::Internal(e.to_string()))?;
            written += chunk.len() as u64;
        }

        file.flush()
            .await
            .map_err(|e| TusError::Internal(e.to_string()))?;

        meta.offset = offset + written;
        self.write_meta_atomic(&meta).await?;

        Ok(written)
    }

    async fn get_upload(&self, id: &str) -> TusResult<UploadInfo> {
        self.ensure_root().await?;
        let meta = self.read_meta(id).await?;
        Ok(meta.into())
    }

    async fn declare_upload_length(&self, id: &str, length: u64) -> TusResult<()> {
        self.ensure_root().await?;
        let mut meta = self.read_meta(id).await?;

        if length < meta.offset {
            return Err(TusError::PayloadTooLarge);
        }

        meta.size = Some(length);
        self.write_meta_atomic(&meta).await?;
        Ok(())
    }
}
