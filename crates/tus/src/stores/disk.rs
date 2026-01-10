use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use salvo_core::async_trait;
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    io::{self, AsyncSeekExt, AsyncWriteExt},
};
use tracing::warn;

use crate::{
    error::{TusError, TusResult},
    handlers::Metadata,
    stores::{ByteStream, DataStore, Extension, StoreInfo, UploadInfo},
};

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
    metadata: Option<HashMap<String, Option<String>>>,
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

#[derive(Clone)]
pub struct DiskStore {
    root: PathBuf,
}

impl DiskStore {
    pub fn new() -> Self {
        Self {
            root: "./tus-upload-files".into(),
        }
    }

    #[allow(dead_code)]
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

    fn resolve_data_path(&self, id: &str, storage: &Option<MetaStoreInfo>) -> PathBuf {
        storage
            .as_ref()
            .map(|info| PathBuf::from(&info.path))
            .unwrap_or_else(|| self.data_path(id))
    }

    fn metadata_value(meta: &MetaUpload, key: &str) -> Option<String> {
        meta.metadata
            .as_ref()
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_ref())
            .map(|v| v.to_string())
    }

    fn sanitize_filename(name: &str) -> Option<String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return None;
        }
        let file_name = Path::new(trimmed)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())?;
        if file_name == "." || file_name == ".." {
            return None;
        }
        Some(file_name)
    }

    fn filename_from_meta(meta: &MetaUpload) -> Option<String> {
        Self::metadata_value(meta, "filename").and_then(|name| Self::sanitize_filename(&name))
    }

    fn extension_from_filetype(meta: &MetaUpload) -> Option<String> {
        let filetype = Self::metadata_value(meta, "filetype")?;
        let subtype = filetype.split('/').nth(1)?.trim();
        if subtype.is_empty() {
            return None;
        }
        Some(subtype.to_string())
    }

    fn desired_filename(meta: &MetaUpload) -> Option<String> {
        if let Some(mut name) = Self::filename_from_meta(meta) {
            if Path::new(&name).extension().is_none() {
                if let Some(ext) = Self::extension_from_filetype(meta) {
                    name = format!("{name}.{ext}");
                }
            }
            return Some(name);
        }
        Self::extension_from_filetype(meta).map(|ext| format!("{}.{}", meta.id, ext))
    }

    fn with_id_suffix(name: &str, id: &str) -> String {
        let path = Path::new(name);
        match (path.file_stem(), path.extension()) {
            (Some(stem), Some(ext)) => {
                format!(
                    "{}-{}.{}",
                    stem.to_string_lossy(),
                    id,
                    ext.to_string_lossy()
                )
            }
            (Some(stem), None) => format!("{}-{}", stem.to_string_lossy(), id),
            _ => format!("{}-{}", name, id),
        }
    }

    async fn try_finalize_if_complete(&self, meta: &mut MetaUpload) {
        let Some(size) = meta.size else {
            return;
        };
        if meta.offset != size {
            return;
        }
        let Some(file_name) = Self::desired_filename(meta) else {
            return;
        };

        let current_path = self.resolve_data_path(&meta.id, &meta.storage);
        let dir = current_path.parent().unwrap_or(self.root.as_path());
        let mut target_path = dir.join(&file_name);
        if target_path == current_path {
            return;
        }

        if fs::metadata(&target_path).await.is_ok() {
            let fallback = Self::with_id_suffix(&file_name, &meta.id);
            target_path = dir.join(fallback);
            if fs::metadata(&target_path).await.is_ok() {
                return;
            }
        }

        if let Err(err) = fs::rename(&current_path, &target_path).await {
            warn!(
                "finalize upload rename failed: id={}, error={}",
                meta.id, err
            );
            return;
        }

        let storage = meta.storage.get_or_insert_with(|| MetaStoreInfo {
            type_name: "file".to_string(),
            path: target_path.to_string_lossy().to_string(),
            bucket: None,
        });
        storage.path = target_path.to_string_lossy().to_string();
    }

    async fn read_meta(&self, id: &str) -> TusResult<MetaUpload> {
        let path = self.meta_path(id);
        let bytes = fs::read(path).await.map_err(|e| {
            match e.kind() {
                io::ErrorKind::NotFound => TusError::NotFound,
                _ => TusError::Internal(e.to_string()),
            }
        })?;
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

#[async_trait]
impl DataStore for DiskStore {
    /// The DiskStore support extensions.
    /// Extension::Creation
    fn extensions(&self) -> HashSet<Extension> {
        let mut support_extensions = HashSet::new();
        support_extensions.insert(Extension::Creation);
        support_extensions.insert(Extension::CreationDeferLength);
        support_extensions.insert(Extension::CreationWithUpload);
        support_extensions.insert(Extension::Termination);
        support_extensions
    }

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

        let mut meta = MetaUpload::from(file.clone());
        self.try_finalize_if_complete(&mut meta).await;
        if let Err(err) = self.write_meta_atomic(&meta).await {
            let _ = fs::remove_file(&data_path).await;
            return Err(err);
        }

        Ok(file)
    }

    async fn remove(&self, id: &str) -> TusResult<()> {
        self.ensure_root().await?;

        let meta = self.read_meta(id).await.ok();
        let data_path = match &meta {
            Some(meta) => self.resolve_data_path(id, &meta.storage),
            None => self.data_path(id),
        };
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

        let path = self.resolve_data_path(id, &meta.storage);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .await
            .map_err(|e| {
                match e.kind() {
                    io::ErrorKind::NotFound => TusError::NotFound,
                    _ => TusError::Internal(e.to_string()),
                }
            })?;

        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| TusError::Internal(e.to_string()))?;

        let original_offset = offset;
        let mut written: u64 = 0;
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| TusError::Internal(e.to_string()))?;
            if let Some(size) = meta.size {
                if original_offset + written + chunk.len() as u64 > size {
                    let _ = file.set_len(original_offset).await;
                    return Err(TusError::PayloadTooLarge);
                }
            }
            file.write_all(&chunk)
                .await
                .map_err(|e| TusError::Internal(e.to_string()))?;
            written += chunk.len() as u64;
        }

        file.flush()
            .await
            .map_err(|e| TusError::Internal(e.to_string()))?;

        meta.offset = original_offset + written;
        self.try_finalize_if_complete(&mut meta).await;
        self.write_meta_atomic(&meta).await?;

        Ok(written)
    }

    async fn get_upload_file_info(&self, id: &str) -> TusResult<UploadInfo> {
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
