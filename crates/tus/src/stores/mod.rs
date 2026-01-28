mod disk;

use std::collections::HashSet;
use std::pin::Pin;

use bytes::Bytes;
pub use disk::*;
use futures_util::Stream;
use salvo_core::async_trait;
use salvo_core::http::HeaderValue;

use crate::error::TusResult;
use crate::handlers::Metadata;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>>;

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
    pub fn get_size_is_deferred(&self) -> bool {
        self.size.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Extension {
    Creation,
    Expiration,
    CreationWithUpload,
    CreationDeferLength,
    Concatenation,
    Termination,
}

impl Extension {
    pub fn as_str(&self) -> &'static str {
        match self {
            Extension::Creation => "creation",
            Extension::Expiration => "expiration",
            Extension::CreationWithUpload => "creation-with-upload",
            Extension::CreationDeferLength => "creation-defer-length",
            Extension::Concatenation => "concatenation",
            Extension::Termination => "termination",
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
/// Feature detection SHOULD be achieved by the Client sending an OPTIONS request and the Server
/// responding with the Tus-Extension header. See more details: https://tus.io/protocols/resumable-upload#protocol-extensions
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
        let cloned = ext.clone();
        let copied = ext;
        assert_eq!(ext, cloned);
        assert_eq!(ext, copied);
    }

    #[test]
    fn test_upload_info_get_size_is_deferred_true() {
        let info = UploadInfo {
            id: "test".to_string(),
            size: None,
            offset: Some(0),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert!(info.get_size_is_deferred());
    }

    #[test]
    fn test_upload_info_get_size_is_deferred_false() {
        let info = UploadInfo {
            id: "test".to_string(),
            size: Some(1024),
            offset: Some(0),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        assert!(!info.get_size_is_deferred());
    }

    #[test]
    fn test_upload_info_clone() {
        let info = UploadInfo {
            id: "test-id".to_string(),
            size: Some(2048),
            offset: Some(512),
            metadata: None,
            storage: Some(StoreInfo {
                type_name: "disk".to_string(),
                path: "/uploads/test.bin".to_string(),
                bucket: None,
            }),
            creation_date: "2024-01-01T00:00:00Z".to_string(),
        };

        let cloned = info.clone();
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
            type_name: "s3".to_string(),
            path: "uploads/file.bin".to_string(),
            bucket: Some("my-bucket".to_string()),
        };

        let cloned = info.clone();
        assert_eq!(cloned.type_name, "s3");
        assert_eq!(cloned.path, "uploads/file.bin");
        assert_eq!(cloned.bucket, Some("my-bucket".to_string()));
    }

    #[test]
    fn test_upload_info_with_metadata() {
        use std::collections::HashMap;

        use crate::handlers::Metadata;

        let mut map = HashMap::new();
        map.insert("filename".to_string(), Some("test.txt".to_string()));

        let info = UploadInfo {
            id: "test".to_string(),
            size: Some(100),
            offset: Some(0),
            metadata: Some(Metadata(map)),
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };

        assert!(info.metadata.is_some());
        let metadata = info.metadata.unwrap();
        assert_eq!(
            metadata.get("filename"),
            Some(&Some("test.txt".to_string()))
        );
    }

    #[test]
    fn test_extension_debug() {
        let ext = Extension::Creation;
        let debug_str = format!("{:?}", ext);
        assert_eq!(debug_str, "Creation");
    }

    #[test]
    fn test_store_info_debug() {
        let info = StoreInfo {
            type_name: "disk".to_string(),
            path: "/uploads/test.bin".to_string(),
            bucket: None,
        };
        let debug_str = format!("{:?}", info);
        assert!(debug_str.contains("disk"));
        assert!(debug_str.contains("/uploads/test.bin"));
    }

    #[test]
    fn test_upload_info_debug() {
        let info = UploadInfo {
            id: "abc123".to_string(),
            size: Some(1024),
            offset: Some(512),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01".to_string(),
        };
        let debug_str = format!("{:?}", info);
        assert!(debug_str.contains("abc123"));
        assert!(debug_str.contains("1024"));
        assert!(debug_str.contains("512"));
    }
}
