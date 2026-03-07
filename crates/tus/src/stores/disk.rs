use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use salvo_core::async_trait;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{self, AsyncSeekExt, AsyncWriteExt};
use tracing::warn;

use crate::error::{TusError, TusResult};
use crate::handlers::Metadata;
use crate::stores::{ByteStream, DataStore, Extension, StoreInfo, UploadInfo};

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
            if Path::new(&name).extension().is_none()
                && let Some(ext) = Self::extension_from_filetype(meta)
            {
                name = format!("{name}.{ext}");
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
        let bytes = fs::read(path).await.map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => TusError::NotFound,
            _ => TusError::Internal(e.to_string()),
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
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => TusError::NotFound,
                _ => TusError::Internal(e.to_string()),
            })?;

        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| TusError::Internal(e.to_string()))?;

        let original_offset = offset;
        let mut written: u64 = 0;
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| TusError::Internal(e.to_string()))?;
            if let Some(size) = meta.size
                && original_offset + written + chunk.len() as u64 > size
            {
                let _ = file.set_len(original_offset).await;
                return Err(TusError::PayloadTooLarge);
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tempfile::TempDir;

    use super::*;

    fn create_test_store() -> (DiskStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = DiskStore::new().disk_root(temp_dir.path());
        (store, temp_dir)
    }

    fn create_test_upload_info(id: &str) -> UploadInfo {
        UploadInfo {
            id: id.to_string(),
            size: Some(1024),
            offset: Some(0),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_disk_store_new() {
        let store = DiskStore::new();
        assert_eq!(store.root, PathBuf::from("./tus-upload-files"));
    }

    #[test]
    fn test_disk_store_disk_root() {
        let store = DiskStore::new().disk_root("/custom/path");
        assert_eq!(store.root, PathBuf::from("/custom/path"));
    }

    #[test]
    fn test_disk_store_data_path() {
        let store = DiskStore::new().disk_root("/uploads");
        let path = store.data_path("test-id");
        assert_eq!(path, PathBuf::from("/uploads/test-id.bin"));
    }

    #[test]
    fn test_disk_store_meta_path() {
        let store = DiskStore::new().disk_root("/uploads");
        let path = store.meta_path("test-id");
        assert_eq!(path, PathBuf::from("/uploads/test-id.json"));
    }

    #[test]
    fn test_disk_store_meta_tmp_path() {
        let store = DiskStore::new().disk_root("/uploads");
        let path = store.meta_tmp_path("test-id");
        assert_eq!(path, PathBuf::from("/uploads/test-id.json.tmp"));
    }

    #[test]
    fn test_disk_store_extensions() {
        let store = DiskStore::new();
        let extensions = store.extensions();

        assert!(extensions.contains(&Extension::Creation));
        assert!(extensions.contains(&Extension::CreationDeferLength));
        assert!(extensions.contains(&Extension::CreationWithUpload));
        assert!(extensions.contains(&Extension::Termination));
        assert!(!extensions.contains(&Extension::Expiration));
        assert!(!extensions.contains(&Extension::Concatenation));
    }

    #[test]
    fn test_disk_store_has_extension() {
        let store = DiskStore::new();

        assert!(store.has_extension(Extension::Creation));
        assert!(store.has_extension(Extension::Termination));
        assert!(!store.has_extension(Extension::Expiration));
    }

    #[test]
    fn test_sanitize_filename_valid() {
        assert_eq!(
            DiskStore::sanitize_filename("test.txt"),
            Some("test.txt".to_string())
        );
        assert_eq!(
            DiskStore::sanitize_filename("  test.txt  "),
            Some("test.txt".to_string())
        );
        assert_eq!(
            DiskStore::sanitize_filename("/path/to/file.txt"),
            Some("file.txt".to_string())
        );
        assert_eq!(DiskStore::sanitize_filename("a"), Some("a".to_string()));
    }

    #[test]
    fn test_sanitize_filename_invalid() {
        assert_eq!(DiskStore::sanitize_filename(""), None);
        assert_eq!(DiskStore::sanitize_filename("   "), None);
        assert_eq!(DiskStore::sanitize_filename("."), None);
        assert_eq!(DiskStore::sanitize_filename(".."), None);
    }

    #[test]
    fn test_filename_from_meta_with_filename() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: 0,
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("filename".to_string(), Some("document.pdf".to_string()));
                m
            }),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert_eq!(
            DiskStore::filename_from_meta(&meta),
            Some("document.pdf".to_string())
        );
    }

    #[test]
    fn test_filename_from_meta_without_filename() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: 0,
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert_eq!(DiskStore::filename_from_meta(&meta), None);
    }

    #[test]
    fn test_extension_from_filetype() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: 0,
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("filetype".to_string(), Some("application/pdf".to_string()));
                m
            }),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert_eq!(
            DiskStore::extension_from_filetype(&meta),
            Some("pdf".to_string())
        );
    }

    #[test]
    fn test_extension_from_filetype_image() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: 0,
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("filetype".to_string(), Some("image/png".to_string()));
                m
            }),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert_eq!(
            DiskStore::extension_from_filetype(&meta),
            Some("png".to_string())
        );
    }

    #[test]
    fn test_extension_from_filetype_invalid() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: 0,
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("filetype".to_string(), Some("invalid".to_string()));
                m
            }),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert_eq!(DiskStore::extension_from_filetype(&meta), None);
    }

    #[test]
    fn test_extension_from_filetype_empty_subtype() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: 0,
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("filetype".to_string(), Some("application/".to_string()));
                m
            }),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert_eq!(DiskStore::extension_from_filetype(&meta), None);
    }

    #[test]
    fn test_desired_filename_with_name_and_extension() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: 0,
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("filename".to_string(), Some("document.pdf".to_string()));
                m
            }),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert_eq!(
            DiskStore::desired_filename(&meta),
            Some("document.pdf".to_string())
        );
    }

    #[test]
    fn test_desired_filename_with_name_without_extension() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: 0,
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("filename".to_string(), Some("document".to_string()));
                m.insert("filetype".to_string(), Some("application/pdf".to_string()));
                m
            }),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert_eq!(
            DiskStore::desired_filename(&meta),
            Some("document.pdf".to_string())
        );
    }

    #[test]
    fn test_desired_filename_only_filetype() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: 0,
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("filetype".to_string(), Some("image/png".to_string()));
                m
            }),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert_eq!(
            DiskStore::desired_filename(&meta),
            Some("test-id.png".to_string())
        );
    }

    #[test]
    fn test_desired_filename_no_metadata() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: 0,
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert_eq!(DiskStore::desired_filename(&meta), None);
    }

    #[test]
    fn test_with_id_suffix_with_extension() {
        assert_eq!(
            DiskStore::with_id_suffix("file.txt", "abc123"),
            "file-abc123.txt"
        );
    }

    #[test]
    fn test_with_id_suffix_without_extension() {
        assert_eq!(DiskStore::with_id_suffix("file", "abc123"), "file-abc123");
    }

    #[test]
    fn test_with_id_suffix_complex_name() {
        assert_eq!(
            DiskStore::with_id_suffix("my.file.name.txt", "abc123"),
            "my.file.name-abc123.txt"
        );
    }

    #[test]
    fn test_resolve_data_path_with_storage() {
        let store = DiskStore::new().disk_root("/uploads");
        let storage = Some(MetaStoreInfo {
            type_name: "file".to_string(),
            path: "/custom/path/file.bin".to_string(),
            bucket: None,
        });
        let path = store.resolve_data_path("test-id", &storage);
        assert_eq!(path, PathBuf::from("/custom/path/file.bin"));
    }

    #[test]
    fn test_resolve_data_path_without_storage() {
        let store = DiskStore::new().disk_root("/uploads");
        let path = store.resolve_data_path("test-id", &None);
        assert_eq!(path, PathBuf::from("/uploads/test-id.bin"));
    }

    #[test]
    fn test_meta_store_info_from_store_info() {
        let store_info = StoreInfo {
            type_name: "disk".to_string(),
            path: "/path/to/file".to_string(),
            bucket: Some("bucket".to_string()),
        };
        let meta_info: MetaStoreInfo = store_info.into();
        assert_eq!(meta_info.type_name, "disk");
        assert_eq!(meta_info.path, "/path/to/file");
        assert_eq!(meta_info.bucket, Some("bucket".to_string()));
    }

    #[test]
    fn test_store_info_from_meta_store_info() {
        let meta_info = MetaStoreInfo {
            type_name: "disk".to_string(),
            path: "/path/to/file".to_string(),
            bucket: None,
        };
        let store_info: StoreInfo = meta_info.into();
        assert_eq!(store_info.type_name, "disk");
        assert_eq!(store_info.path, "/path/to/file");
        assert_eq!(store_info.bucket, None);
    }

    #[test]
    fn test_meta_upload_from_upload_info() {
        let upload_info = UploadInfo {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: Some(512),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        let meta: MetaUpload = upload_info.into();
        assert_eq!(meta.id, "test-id");
        assert_eq!(meta.size, Some(1024));
        assert_eq!(meta.offset, 512);
    }

    #[test]
    fn test_meta_upload_from_upload_info_no_offset() {
        let upload_info = UploadInfo {
            id: "test-id".to_string(),
            size: Some(1024),
            offset: None,
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        let meta: MetaUpload = upload_info.into();
        assert_eq!(meta.offset, 0); // defaults to 0
    }

    #[test]
    fn test_upload_info_from_meta_upload() {
        let meta = MetaUpload {
            id: "test-id".to_string(),
            size: Some(2048),
            offset: 1024,
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        let upload_info: UploadInfo = meta.into();
        assert_eq!(upload_info.id, "test-id");
        assert_eq!(upload_info.size, Some(2048));
        assert_eq!(upload_info.offset, Some(1024));
    }

    #[tokio::test]
    async fn test_disk_store_create_and_read() {
        let (store, _temp_dir) = create_test_store();
        let upload_info = create_test_upload_info("test-create-1");

        let created = store.create(upload_info.clone()).await.unwrap();
        assert_eq!(created.id, "test-create-1");
        assert!(created.storage.is_some());

        let info = store.get_upload_file_info("test-create-1").await.unwrap();
        assert_eq!(info.id, "test-create-1");
        assert_eq!(info.size, Some(1024));
        assert_eq!(info.offset, Some(0));
    }

    #[tokio::test]
    async fn test_disk_store_create_with_deferred_size() {
        let (store, _temp_dir) = create_test_store();
        let upload_info = UploadInfo {
            id: "test-deferred".to_string(),
            size: None, // deferred
            offset: Some(0),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };

        let _created = store.create(upload_info).await.unwrap();
        let info = store.get_upload_file_info("test-deferred").await.unwrap();
        assert!(info.get_size_is_deferred());
    }

    #[tokio::test]
    async fn test_disk_store_remove() {
        let (store, _temp_dir) = create_test_store();
        let upload_info = create_test_upload_info("test-remove");

        store.create(upload_info).await.unwrap();
        store.remove("test-remove").await.unwrap();

        let result = store.get_upload_file_info("test-remove").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TusError::NotFound));
    }

    #[tokio::test]
    async fn test_disk_store_remove_not_found() {
        let (store, _temp_dir) = create_test_store();

        let result = store.remove("non-existent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TusError::NotFound));
    }

    #[tokio::test]
    async fn test_disk_store_get_upload_file_info_not_found() {
        let (store, _temp_dir) = create_test_store();

        let result = store.get_upload_file_info("non-existent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TusError::NotFound));
    }

    #[tokio::test]
    async fn test_disk_store_declare_upload_length() {
        let (store, _temp_dir) = create_test_store();
        let upload_info = UploadInfo {
            id: "test-declare".to_string(),
            size: None,
            offset: Some(0),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };

        store.create(upload_info).await.unwrap();

        store
            .declare_upload_length("test-declare", 2048)
            .await
            .unwrap();

        let info = store.get_upload_file_info("test-declare").await.unwrap();
        assert_eq!(info.size, Some(2048));
    }

    #[tokio::test]
    async fn test_disk_store_declare_upload_length_too_small() {
        let (store, _temp_dir) = create_test_store();
        let upload_info = UploadInfo {
            id: "test-declare-small".to_string(),
            size: None,
            offset: Some(100), // Already has some offset
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };

        store.create(upload_info).await.unwrap();

        // Try to declare length smaller than current offset
        let result = store.declare_upload_length("test-declare-small", 50).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TusError::PayloadTooLarge));
    }

    #[tokio::test]
    async fn test_disk_store_write() {
        use bytes::Bytes;
        use futures_util::stream;

        let (store, _temp_dir) = create_test_store();
        let upload_info = UploadInfo {
            id: "test-write".to_string(),
            size: Some(100),
            offset: Some(0),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };

        store.create(upload_info).await.unwrap();

        let data = Bytes::from("Hello, World!");
        let stream: ByteStream =
            Box::pin(stream::once(async move { Ok::<_, std::io::Error>(data) }));

        let written = store.write("test-write", 0, stream).await.unwrap();
        assert_eq!(written, 13);

        let info = store.get_upload_file_info("test-write").await.unwrap();
        assert_eq!(info.offset, Some(13));
    }

    #[tokio::test]
    async fn test_disk_store_write_offset_mismatch() {
        use bytes::Bytes;
        use futures_util::stream;

        let (store, _temp_dir) = create_test_store();
        let upload_info = create_test_upload_info("test-write-mismatch");

        store.create(upload_info).await.unwrap();

        let data = Bytes::from("test");
        let stream: ByteStream =
            Box::pin(stream::once(async move { Ok::<_, std::io::Error>(data) }));

        // Try to write at wrong offset
        let result = store.write("test-write-mismatch", 100, stream).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            TusError::OffsetMismatch { expected, got } => {
                assert_eq!(expected, 0);
                assert_eq!(got, 100);
            }
            _ => panic!("Expected OffsetMismatch error"),
        }
    }

    #[tokio::test]
    async fn test_disk_store_write_payload_too_large() {
        use bytes::Bytes;
        use futures_util::stream;

        let (store, _temp_dir) = create_test_store();
        let upload_info = UploadInfo {
            id: "test-write-large".to_string(),
            size: Some(5), // Only 5 bytes allowed
            offset: Some(0),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };

        store.create(upload_info).await.unwrap();

        let data = Bytes::from("This is too long");
        let stream: ByteStream =
            Box::pin(stream::once(async move { Ok::<_, std::io::Error>(data) }));

        let result = store.write("test-write-large", 0, stream).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TusError::PayloadTooLarge));
    }

    #[tokio::test]
    async fn test_disk_store_write_not_found() {
        use bytes::Bytes;
        use futures_util::stream;

        let (store, _temp_dir) = create_test_store();

        let data = Bytes::from("test");
        let stream: ByteStream =
            Box::pin(stream::once(async move { Ok::<_, std::io::Error>(data) }));

        let result = store.write("non-existent", 0, stream).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TusError::NotFound));
    }

    #[tokio::test]
    async fn test_disk_store_write_multiple_chunks() {
        use bytes::Bytes;
        use futures_util::stream;

        let (store, _temp_dir) = create_test_store();
        let upload_info = UploadInfo {
            id: "test-multi-chunk".to_string(),
            size: Some(100),
            offset: Some(0),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };

        store.create(upload_info).await.unwrap();

        // First write
        let data1 = Bytes::from("Hello, ");
        let stream1: ByteStream =
            Box::pin(stream::once(async move { Ok::<_, std::io::Error>(data1) }));
        let written1 = store.write("test-multi-chunk", 0, stream1).await.unwrap();
        assert_eq!(written1, 7);

        // Second write
        let data2 = Bytes::from("World!");
        let stream2: ByteStream =
            Box::pin(stream::once(async move { Ok::<_, std::io::Error>(data2) }));
        let written2 = store.write("test-multi-chunk", 7, stream2).await.unwrap();
        assert_eq!(written2, 6);

        let info = store
            .get_upload_file_info("test-multi-chunk")
            .await
            .unwrap();
        assert_eq!(info.offset, Some(13));
    }

    #[tokio::test]
    async fn test_disk_store_finalize_on_complete() {
        use bytes::Bytes;
        use futures_util::stream;

        let (store, temp_dir) = create_test_store();

        let mut metadata = HashMap::new();
        metadata.insert("filename".to_string(), Some("myfile.txt".to_string()));

        let upload_info = UploadInfo {
            id: "test-finalize".to_string(),
            size: Some(5),
            offset: Some(0),
            metadata: Some(Metadata(metadata)),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };

        store.create(upload_info).await.unwrap();

        let data = Bytes::from("hello");
        let stream: ByteStream =
            Box::pin(stream::once(async move { Ok::<_, std::io::Error>(data) }));
        store.write("test-finalize", 0, stream).await.unwrap();

        // Check that the file was renamed
        let info = store.get_upload_file_info("test-finalize").await.unwrap();
        assert!(info.storage.is_some());
        let storage = info.storage.unwrap();
        assert!(storage.path.contains("myfile.txt"));

        // Verify file exists at final path
        assert!(temp_dir.path().join("myfile.txt").exists());
    }

    #[tokio::test]
    async fn test_disk_store_create_with_metadata() {
        let (store, _temp_dir) = create_test_store();

        let mut metadata = HashMap::new();
        metadata.insert("filename".to_string(), Some("test.pdf".to_string()));
        metadata.insert("filetype".to_string(), Some("application/pdf".to_string()));

        let upload_info = UploadInfo {
            id: "test-metadata".to_string(),
            size: Some(1024),
            offset: Some(0),
            metadata: Some(Metadata(metadata)),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };

        store.create(upload_info).await.unwrap();

        let info = store.get_upload_file_info("test-metadata").await.unwrap();
        assert!(info.metadata.is_some());
        let meta = info.metadata.unwrap();
        assert_eq!(meta.get("filename"), Some(&Some("test.pdf".to_string())));
    }

    #[test]
    fn test_metadata_value_extraction() {
        let meta = MetaUpload {
            id: "test".to_string(),
            size: Some(100),
            offset: 0,
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("key1".to_string(), Some("value1".to_string()));
                m.insert("key2".to_string(), None);
                m
            }),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };

        assert_eq!(
            DiskStore::metadata_value(&meta, "key1"),
            Some("value1".to_string())
        );
        assert_eq!(DiskStore::metadata_value(&meta, "key2"), None);
        assert_eq!(DiskStore::metadata_value(&meta, "key3"), None);
    }

    #[test]
    fn test_disk_store_clone() {
        let store1 = DiskStore::new().disk_root("/path1");
        let store2 = store1.clone();
        assert_eq!(store1.root, store2.root);
    }
}
