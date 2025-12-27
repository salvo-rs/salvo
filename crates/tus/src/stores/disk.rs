use std::path::PathBuf;

use salvo_core::async_trait;
use tokio::{fs, io::{self}};

use crate::{error::{TusError, TusResult}, stores::DataStore};

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

impl DiskStore {
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

    async fn read_meta(&self, id: &str) -> TusResult<MetaFile> {
        let path = self.meta_path(id);
        let bytes = fs::read(path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => TusError::NotFound,
                _ => TusError::Internal(e.to_string()),
            })?;

        serde_json::from_slice::<MetaFile>(&bytes)
            .map_err(|e| TusError::Internal(format!("invalid meta json: {e}")))
    }

    async fn write_meta_atomic(&self, meta: &MetaFile) -> TusResult<()> {
        let id = &meta.id;
        let tmp = self.meta_tmp_path(id);
        let final_path = self.meta_path(id);

        let json = serde_json::to_vec(meta)
            .map_err(|e| TusError::Internal(format!("serialize meta json: {e}")))?;

        // write tmp
        fs::write(&tmp, json)
            .await
            .map_err(|e| TusError::Internal(e.to_string()))?;

        // atomic-ish replace
        // On Unix rename is atomic. On Windows, rename fails if destination exists.
        // We'll remove then rename to be safe across platforms.
        #[cfg(windows)]
        {
            let _ = fs::remove_file(&final_path).await;
        }

        fs::rename(&tmp, &final_path)
            .await
            .map_err(|e| TusError::Internal(format!("rename meta tmp: {e}")))?;

        Ok(())
    }

    async fn create_empty_data_file(&self, id: &str) -> TusResult<()> {
        let path = self.data_path(id);
        // create new file (overwrite if exists, which is extremely unlikely since UUID)
        let _f = fs::File::create(path)
            .await
            .map_err(|e| TusError::Internal(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl DataStore for DiskStore {
    // async fn create(&self, new: NewUpload) -> TusResult<UploadInfo> {
    //     self.ensure_root().await?;

    //     let id = Uuid::new_v4().to_string();

    //     // 1) data file
    //     self.create_empty_data_file(&id).await?;

    //     // 2) meta file
    //     let meta = MetaFile {
    //         id: id.clone(),
    //         length: new.length,
    //         offset: 0,
    //         metadata: new.metadata,
    //     };
    //     self.write_meta_atomic(&meta).await?;

    //     Ok(meta.into())
    // }

    // async fn get(&self, id: &str) -> TusResult<UploadInfo> {
    //     self.ensure_root().await?;
    //     let meta = self.read_meta(id).await?;
    //     Ok(meta.into())
    // }

    // async fn set_offset(&self, id: &str, offset: u64) -> TusResult<()> {
    //     self.ensure_root().await?;

    //     let mut meta = self.read_meta(id).await?;

    //     if offset < meta.offset {
    //         return Err(TusError::Internal(format!(
    //             "offset must be monotonically increasing: {} -> {}",
    //             meta.offset, offset
    //         )));
    //     }

    //     if offset > meta.length {
    //         return Err(TusError::PayloadTooLarge);
    //     }

    //     meta.offset = offset;
    //     self.write_meta_atomic(&meta).await?;
    //     Ok(())
    // }

    // async fn write(&self, id: &str, offset: u64, mut stream: ByteStream) -> TusResult<u64> {
    //     use futures_util::StreamExt;
        
    //     self.ensure_root().await?;

    //     let path = self.data_path(id);

    //     // Open with write=true. We need seek, so use tokio::fs::File
    //     let mut f = fs::OpenOptions::new()
    //         .write(true)
    //         .open(path)
    //         .await
    //         .map_err(|e| match e.kind() {
    //             io::ErrorKind::NotFound => TusError::NotFound,
    //             _ => TusError::Internal(e.to_string()),
    //         })?;

    //     // Seek to offset
    //     use tokio::io::AsyncSeekExt;
    //     use std::io::SeekFrom;
    //     f.seek(SeekFrom::Start(offset))
    //         .await
    //         .map_err(|e| TusError::Internal(e.to_string()))?;

    //     let mut written: u64 = 0;

    //     while let Some(item) = stream.next().await {
    //         let chunk: Bytes = item.map_err(|e| TusError::Internal(e.to_string()))?;
    //         f.write_all(&chunk)
    //             .await
    //             .map_err(|e| TusError::Internal(e.to_string()))?;
    //         written += chunk.len() as u64;
    //     }

    //     f.flush()
    //         .await
    //         .map_err(|e| TusError::Internal(e.to_string()))?;

    //     Ok(written)
    // }
}