mod disk;

use std::collections::HashSet;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bytes::Bytes;
pub use disk::*;
use futures_util::{Stream, StreamExt};
use salvo_core::async_trait;
use salvo_core::http::HeaderValue;

use crate::error::{TusError, TusResult};
use crate::handlers::Metadata;

/// Async byte stream consumed by storage backends when writing upload chunks.
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>>;
const UPLOAD_SIZE_LIMIT_EXCEEDED: &str = "tus upload size limit exceeded";

fn upload_size_limit_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, UPLOAD_SIZE_LIMIT_EXCEEDED)
}

fn is_upload_size_limit_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::InvalidData && error.to_string() == UPLOAD_SIZE_LIMIT_EXCEEDED
}

fn limit_stream(
    stream: ByteStream,
    max_bytes: Option<u64>,
    exceeded: Arc<AtomicBool>,
) -> ByteStream {
    let Some(max_bytes) = max_bytes else {
        return stream;
    };

    let mut received = 0u64;
    Box::pin(stream.map(move |item| {
        let chunk = item?;
        let Some(next) = received.checked_add(chunk.len() as u64) else {
            exceeded.store(true, Ordering::Relaxed);
            return Err(upload_size_limit_error());
        };
        if next > max_bytes {
            exceeded.store(true, Ordering::Relaxed);
            return Err(upload_size_limit_error());
        }
        received = next;
        Ok(chunk)
    }))
}

// #[derive(Debug, Clone)]
// pub enum StoreType {
//     Disk,
// }

/// Storage location metadata for an upload.
#[derive(Debug, Clone)]
pub struct StoreInfo {
    /// Storage backend type name, such as `file`.
    pub type_name: String,
    /// Backend-specific path or object key for the upload.
    pub path: String,
    /// Optional storage bucket name for object-storage backends.
    pub bucket: Option<String>,
}
/// Metadata describing a tus upload.
#[derive(Debug, Clone)]
pub struct UploadInfo {
    /// Upload ID.
    pub id: String,
    /// Total upload size, or `None` when the client deferred the length.
    pub size: Option<u64>,
    /// Current upload offset in bytes.
    pub offset: Option<u64>,
    /// Optional tus `Upload-Metadata` values.
    pub metadata: Option<Metadata>,
    /// Storage location for the upload data.
    pub storage: Option<StoreInfo>,
    /// Upload creation timestamp.
    pub creation_date: String,
}

impl UploadInfo {
    /// Returns `true` when the upload length has not been declared yet.
    #[must_use]
    pub fn is_size_deferred(&self) -> bool {
        self.size.is_none()
    }
}

/// Tus protocol extension advertised by a storage backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Extension {
    /// Upload creation extension.
    Creation,
    /// Upload expiration extension.
    Expiration,
    /// Creation-with-upload extension.
    CreationWithUpload,
    /// Creation-defer-length extension.
    CreationDeferLength,
    /// Concatenation extension.
    Concatenation,
    /// Termination extension.
    Termination,
}

impl Extension {
    /// Returns the tus header token for this extension.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Creation => "creation",
            Self::Expiration => "expiration",
            Self::CreationWithUpload => "creation-with-upload",
            Self::CreationDeferLength => "creation-defer-length",
            Self::Concatenation => "concatenation",
            Self::Termination => "termination",
        }
    }

    /// Converts a set of extensions into a `Tus-Extension` header value.
    #[must_use]
    pub fn to_header_value(extensions: &HashSet<Self>) -> Option<HeaderValue> {
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

/// Store-supported tus protocol extensions.
///
/// The default extension set is empty. Clients and servers are encouraged to
/// implement as many extensions as possible. Feature detection should use an
/// `OPTIONS` request and a `Tus-Extension` response header.
///
/// See the tus protocol extension docs:
/// <https://tus.io/protocols/resumable-upload#protocol-extensions>
#[async_trait]
pub trait DataStore: Send + Sync + 'static {
    /// Returns the tus extensions supported by this storage backend.
    fn extensions(&self) -> HashSet<Extension> {
        HashSet::new()
    }

    /// Returns `true` when this backend supports `ext`.
    fn has_extension(&self, ext: Extension) -> bool {
        self.extensions().contains(&ext)
    }

    /// Creates metadata and storage for a new upload.
    async fn create(&self, file: UploadInfo) -> TusResult<UploadInfo>;
    /// Removes an upload and its metadata.
    async fn remove(&self, id: &str) -> TusResult<()>;
    /// Writes a chunk stream at `offset` and returns the number of bytes written.
    async fn write(&self, id: &str, offset: u64, stream: ByteStream) -> TusResult<u64>;
    /// Writes a chunk stream while enforcing an optional maximum byte count.
    async fn write_limited(
        &self,
        id: &str,
        offset: u64,
        stream: ByteStream,
        max_bytes: Option<u64>,
    ) -> TusResult<u64> {
        let exceeded = Arc::new(AtomicBool::new(false));
        let stream = limit_stream(stream, max_bytes, exceeded.clone());
        match self.write(id, offset, stream).await {
            Err(_) if exceeded.load(Ordering::Relaxed) => Err(TusError::PayloadTooLarge),
            result => result,
        }
    }
    /// Returns metadata for an existing upload.
    async fn get_upload_file_info(&self, id: &str) -> TusResult<UploadInfo>;
    /// Declares the final upload length for an upload created with deferred length.
    async fn declare_upload_length(&self, id: &str, length: u64) -> TusResult<()>;

    /// Deletes expired uploads and returns the number of removed uploads.
    async fn delete_expired(&self) -> TusResult<u32> {
        Ok(0)
    }
    /// Returns the upload expiration duration configured by this backend.
    fn get_expiration(&self) -> Option<std::time::Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_as_str() {
        assert_eq!(Extension::Creation.as_str(), "creation");
        assert_eq!(Extension::Expiration.as_str(), "expiration");
        assert_eq!(
            Extension::CreationWithUpload.as_str(),
            "creation-with-upload"
        );
        assert_eq!(
            Extension::CreationDeferLength.as_str(),
            "creation-defer-length"
        );
        assert_eq!(Extension::Concatenation.as_str(), "concatenation");
        assert_eq!(Extension::Termination.as_str(), "termination");
    }

    #[test]
    fn test_extension_to_header_value_empty() {
        let extensions = HashSet::new();
        assert!(Extension::to_header_value(&extensions).is_none());
    }

    #[test]
    fn test_extension_to_header_value_single() {
        let mut extensions = HashSet::new();
        extensions.insert(Extension::Creation);
        let header = Extension::to_header_value(&extensions).unwrap();
        assert_eq!(header.to_str().unwrap(), "creation");
    }

    #[test]
    fn test_extension_to_header_value_multiple() {
        let mut extensions = HashSet::new();
        extensions.insert(Extension::Creation);
        extensions.insert(Extension::Termination);
        let header = Extension::to_header_value(&extensions).unwrap();
        let value = header.to_str().unwrap();
        // Order may vary in HashSet
        assert!(value.contains("creation"));
        assert!(value.contains("termination"));
        assert!(value.contains(","));
    }

    #[test]
    fn test_extension_equality() {
        assert_eq!(Extension::Creation, Extension::Creation);
        assert_ne!(Extension::Creation, Extension::Termination);
    }

    #[test]
    fn test_extension_hash() {
        let mut set = HashSet::new();
        set.insert(Extension::Creation);
        set.insert(Extension::Creation); // duplicate
        assert_eq!(set.len(), 1);

        set.insert(Extension::Termination);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_extension_clone_and_copy() {
        let ext = Extension::Creation;
        let cloned = ext;
        let copied = ext;
        assert_eq!(ext, cloned);
        assert_eq!(ext, copied);
    }

    #[test]
    fn test_upload_info_is_size_deferred_true() {
        let info = UploadInfo {
            id: "test".to_owned(),
            size: None,
            offset: Some(0),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_owned(),
        };
        assert!(info.is_size_deferred());
    }

    #[test]
    fn test_upload_info_is_size_deferred_false() {
        let info = UploadInfo {
            id: "test".to_owned(),
            size: Some(1024),
            offset: Some(0),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_owned(),
        };
        assert!(!info.is_size_deferred());
    }

    #[test]
    fn test_upload_info_clone() {
        let info = UploadInfo {
            id: "test-id".to_owned(),
            size: Some(2048),
            offset: Some(512),
            metadata: None,
            storage: Some(StoreInfo {
                type_name: "disk".to_owned(),
                path: "/uploads/test.bin".to_owned(),
                bucket: None,
            }),
            creation_date: "2024-01-01T00:00:00Z".to_owned(),
        };

        let cloned = info;
        assert_eq!(cloned.id, "test-id");
        assert_eq!(cloned.size, Some(2048));
        assert_eq!(cloned.offset, Some(512));
        assert!(cloned.storage.is_some());
        let storage = cloned.storage.unwrap();
        assert_eq!(storage.type_name, "disk");
        assert_eq!(storage.path, "/uploads/test.bin");
    }

    #[test]
    fn test_store_info_clone() {
        let info = StoreInfo {
            type_name: "s3".to_owned(),
            path: "uploads/file.bin".to_owned(),
            bucket: Some("my-bucket".to_owned()),
        };

        let cloned = info;
        assert_eq!(cloned.type_name, "s3");
        assert_eq!(cloned.path, "uploads/file.bin");
        assert_eq!(cloned.bucket, Some("my-bucket".to_owned()));
    }

    #[test]
    fn test_upload_info_with_metadata() {
        use std::collections::HashMap;

        use crate::handlers::Metadata;

        let mut map = HashMap::new();
        map.insert("filename".to_owned(), Some("test.txt".to_owned()));

        let info = UploadInfo {
            id: "test".to_owned(),
            size: Some(100),
            offset: Some(0),
            metadata: Some(Metadata(map)),
            storage: None,
            creation_date: "2024-01-01".to_owned(),
        };

        assert!(info.metadata.is_some());
        let metadata = info.metadata.unwrap();
        assert_eq!(metadata.get("filename"), Some(&Some("test.txt".to_owned())));
    }

    #[test]
    fn test_extension_debug() {
        let ext = Extension::Creation;
        let debug_str = format!("{ext:?}");
        assert_eq!(debug_str, "Creation");
    }

    #[test]
    fn test_store_info_debug() {
        let info = StoreInfo {
            type_name: "disk".to_owned(),
            path: "/uploads/test.bin".to_owned(),
            bucket: None,
        };
        let debug_str = format!("{info:?}");
        assert!(debug_str.contains("disk"));
        assert!(debug_str.contains("/uploads/test.bin"));
    }

    #[test]
    fn test_upload_info_debug() {
        let info = UploadInfo {
            id: "abc123".to_owned(),
            size: Some(1024),
            offset: Some(512),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_owned(),
        };
        let debug_str = format!("{info:?}");
        assert!(debug_str.contains("abc123"));
        assert!(debug_str.contains("1024"));
        assert!(debug_str.contains("512"));
    }
}
