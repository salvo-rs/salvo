mod base;
mod delete;
mod get;
mod head;
mod options;
mod patch;
mod post;

use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};

use base64::Engine;
pub use delete::delete_handler;
pub use get::get_handler;
pub use head::head_handler;
pub use options::options_handler;
pub use patch::patch_handler;
pub use post::post_handler;
use salvo_core::http::{HeaderMap, HeaderValue};

use crate::error::ProtocolError;
use crate::{H_TUS_RESUMABLE, TUS_VERSION};

pub(crate) const EXPOSE_HEADERS: &str = "Location, Upload-Offset, Upload-Length, Upload-Metadata, Upload-Expires, Tus-Resumable, Tus-Version, Tus-Extension, Tus-Max-Size";

pub(crate) fn apply_common_headers(headers: &mut HeaderMap) -> &mut HeaderMap {
    headers.insert(H_TUS_RESUMABLE, HeaderValue::from_static(TUS_VERSION));
    headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    headers.insert(
        "access-control-expose-headers",
        HeaderValue::from_static(EXPOSE_HEADERS),
    );
    headers.insert("cache-control", HeaderValue::from_static("no-store"));

    headers
}

pub(crate) fn apply_options_headers(headers: &mut HeaderMap) -> &mut HeaderMap {
    headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    headers.insert(
        "access-control-expose-headers",
        HeaderValue::from_static(EXPOSE_HEADERS),
    );
    headers.insert("cache-control", HeaderValue::from_static("no-store"));

    headers
}

#[derive(Clone, Debug, Default)]
pub struct Metadata(pub HashMap<String, Option<String>>);

impl Metadata {
    pub fn parse_metadata(raw: &str) -> Result<Metadata, ProtocolError> {
        if raw.trim().is_empty() {
            return Err(ProtocolError::InvalidMetadata);
        }

        let mut map = HashMap::new();
        let mut seen = HashSet::new();

        for item in raw.split(',') {
            let tokens: Vec<&str> = item.split(' ').collect();
            if tokens.is_empty() || tokens.len() > 2 {
                return Err(ProtocolError::InvalidMetadata);
            }

            let key = tokens[0];
            if !validate_key(key) || !seen.insert(key.to_string()) {
                return Err(ProtocolError::InvalidMetadata);
            }

            if tokens.len() == 1 {
                map.insert(key.to_string(), None);
                continue;
            }

            let value = tokens[1];
            if !validate_value(value) {
                return Err(ProtocolError::InvalidMetadata);
            }

            let decoded = base64::engine::general_purpose::STANDARD
                .decode(value)
                .map_err(|_| ProtocolError::InvalidMetadata)?;
            let decoded_value = String::from_utf8_lossy(&decoded).to_string();

            map.insert(key.to_string(), Some(decoded_value));
        }

        Ok(Metadata(map))
    }

    pub fn stringify(metadata: Metadata) -> String {
        metadata
            .0
            .iter()
            .map(|(key, value)| match value {
                Some(value) => {
                    let encoded =
                        base64::engine::general_purpose::STANDARD.encode(value.as_bytes());
                    format!("{} {}", key, encoded)
                }
                None => key.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn validate_key(key: &str) -> bool {
    !key.is_empty() && !key.contains(' ') && !key.contains(',')
}

fn validate_value(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .is_ok()
}

impl Deref for Metadata {
    type Target = HashMap<String, Option<String>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Metadata {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GenerateUrlCtx<'a> {
    pub proto: &'a str,
    pub host: &'a str,
    pub path: &'a str,
    pub id: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct HostProto<'a> {
    pub proto: &'a str,
    pub host: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_parse_single_key_value() {
        let raw = "filename dGVzdC50eHQ="; // "test.txt" in base64
        let metadata = Metadata::parse_metadata(raw).unwrap();
        assert_eq!(
            metadata.get("filename"),
            Some(&Some("test.txt".to_string()))
        );
    }

    #[test]
    fn test_metadata_parse_multiple_key_values() {
        let raw = "filename dGVzdC50eHQ=,filetype dGV4dC9wbGFpbg=="; // "test.txt", "text/plain"
        let metadata = Metadata::parse_metadata(raw).unwrap();
        assert_eq!(
            metadata.get("filename"),
            Some(&Some("test.txt".to_string()))
        );
        assert_eq!(
            metadata.get("filetype"),
            Some(&Some("text/plain".to_string()))
        );
    }

    #[test]
    fn test_metadata_parse_key_without_value() {
        let raw = "is_private";
        let metadata = Metadata::parse_metadata(raw).unwrap();
        assert_eq!(metadata.get("is_private"), Some(&None));
    }

    #[test]
    fn test_metadata_parse_mixed_keys() {
        let raw = "filename dGVzdC50eHQ=,is_private,size MTAyNA=="; // "test.txt", no value, "1024"
        let metadata = Metadata::parse_metadata(raw).unwrap();
        assert_eq!(
            metadata.get("filename"),
            Some(&Some("test.txt".to_string()))
        );
        assert_eq!(metadata.get("is_private"), Some(&None));
        assert_eq!(metadata.get("size"), Some(&Some("1024".to_string())));
    }

    #[test]
    fn test_metadata_parse_empty_string() {
        let result = Metadata::parse_metadata("");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProtocolError::InvalidMetadata
        ));
    }

    #[test]
    fn test_metadata_parse_whitespace_only() {
        let result = Metadata::parse_metadata("   ");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProtocolError::InvalidMetadata
        ));
    }

    #[test]
    fn test_metadata_parse_empty_key() {
        let result = Metadata::parse_metadata(" dGVzdA=="); // space at beginning creates empty key
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_parse_key_with_space() {
        // Keys cannot contain spaces
        let result = Metadata::parse_metadata("file name dGVzdA==");
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_parse_key_with_comma() {
        // Commas are used as separators - "file,name dGVzdA==" is parsed as:
        // - "file" (key without value)
        // - "name dGVzdA==" (key "name" with base64 value)
        let raw = "file,name dGVzdA==";
        let result = Metadata::parse_metadata(raw).unwrap();
        assert_eq!(result.get("file"), Some(&None));
        assert_eq!(result.get("name"), Some(&Some("test".to_string())));
    }

    #[test]
    fn test_metadata_parse_duplicate_keys() {
        let raw = "filename dGVzdDE=,filename dGVzdDI="; // "test1", "test2"
        let result = Metadata::parse_metadata(raw);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProtocolError::InvalidMetadata
        ));
    }

    #[test]
    fn test_metadata_parse_invalid_base64() {
        let raw = "filename !!!invalid!!!";
        let result = Metadata::parse_metadata(raw);
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_parse_too_many_tokens() {
        let raw = "key value extra";
        let result = Metadata::parse_metadata(raw);
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_stringify_single_key_value() {
        let mut metadata = Metadata::default();
        metadata.insert("filename".to_string(), Some("test.txt".to_string()));
        let result = Metadata::stringify(metadata);
        assert_eq!(result, "filename dGVzdC50eHQ=");
    }

    #[test]
    fn test_metadata_stringify_key_without_value() {
        let mut metadata = Metadata::default();
        metadata.insert("is_private".to_string(), None);
        let result = Metadata::stringify(metadata);
        assert_eq!(result, "is_private");
    }

    #[test]
    fn test_metadata_stringify_multiple_keys() {
        let mut metadata = Metadata::default();
        metadata.insert("filename".to_string(), Some("test.txt".to_string()));
        metadata.insert("is_private".to_string(), None);
        let result = Metadata::stringify(metadata);
        // Order may vary due to HashMap, so check both parts are present
        assert!(result.contains("filename dGVzdC50eHQ="));
        assert!(result.contains("is_private"));
        assert!(result.contains(", "));
    }

    #[test]
    fn test_metadata_stringify_empty() {
        let metadata = Metadata::default();
        let result = Metadata::stringify(metadata);
        assert_eq!(result, "");
    }

    #[test]
    fn test_metadata_roundtrip() {
        let original = "filename dGVzdC50eHQ=";
        let parsed = Metadata::parse_metadata(original).unwrap();
        let stringified = Metadata::stringify(parsed);
        assert_eq!(stringified, original);
    }

    #[test]
    fn test_metadata_deref() {
        let mut metadata = Metadata::default();
        metadata.insert("key".to_string(), Some("value".to_string()));

        // Test Deref
        assert!(metadata.contains_key("key"));
        assert_eq!(metadata.len(), 1);

        // Test DerefMut
        metadata.insert("key2".to_string(), None);
        assert_eq!(metadata.len(), 2);
    }

    #[test]
    fn test_metadata_parse_utf8_value() {
        // "文件" (Chinese for "file") in base64
        let raw = "name 5paH5Lu2";
        let metadata = Metadata::parse_metadata(raw).unwrap();
        assert_eq!(metadata.get("name"), Some(&Some("文件".to_string())));
    }

    #[test]
    fn test_metadata_parse_special_characters_in_value() {
        // "hello\nworld" in base64
        let raw = "content aGVsbG8Kd29ybGQ=";
        let metadata = Metadata::parse_metadata(raw).unwrap();
        assert_eq!(
            metadata.get("content"),
            Some(&Some("hello\nworld".to_string()))
        );
    }

    #[test]
    fn test_validate_key_valid() {
        assert!(validate_key("filename"));
        assert!(validate_key("file-name"));
        assert!(validate_key("file_name"));
        assert!(validate_key("fileName123"));
        assert!(validate_key("a"));
    }

    #[test]
    fn test_validate_key_invalid() {
        assert!(!validate_key(""));
        assert!(!validate_key("file name")); // contains space
        assert!(!validate_key("file,name")); // contains comma
    }

    #[test]
    fn test_validate_value_valid() {
        assert!(validate_value("dGVzdA==")); // "test"
        assert!(validate_value("aGVsbG8=")); // "hello"
        assert!(validate_value("YQ==")); // "a"
    }

    #[test]
    fn test_validate_value_invalid() {
        assert!(!validate_value("")); // empty
        assert!(!validate_value("!!!")); // invalid base64
        assert!(!validate_value("not base64!")); // invalid base64
    }

    #[test]
    fn test_apply_common_headers() {
        let mut headers = HeaderMap::new();
        apply_common_headers(&mut headers);

        assert_eq!(headers.get(H_TUS_RESUMABLE).unwrap(), TUS_VERSION);
        assert_eq!(headers.get("access-control-allow-origin").unwrap(), "*");
        assert!(
            headers
                .get("access-control-expose-headers")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("Upload-Offset")
        );
        assert_eq!(headers.get("cache-control").unwrap(), "no-store");
    }

    #[test]
    fn test_apply_options_headers() {
        let mut headers = HeaderMap::new();
        apply_options_headers(&mut headers);

        assert_eq!(headers.get("access-control-allow-origin").unwrap(), "*");
        assert!(
            headers
                .get("access-control-expose-headers")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("Tus-Resumable")
        );
        assert_eq!(headers.get("cache-control").unwrap(), "no-store");
        // Should NOT have tus-resumable header
        assert!(headers.get(H_TUS_RESUMABLE).is_none());
    }

    #[test]
    fn test_generate_url_ctx_fields() {
        let ctx = GenerateUrlCtx {
            proto: "https",
            host: "example.com",
            path: "/uploads",
            id: "abc123",
        };
        assert_eq!(ctx.proto, "https");
        assert_eq!(ctx.host, "example.com");
        assert_eq!(ctx.path, "/uploads");
        assert_eq!(ctx.id, "abc123");
    }

    #[test]
    fn test_host_proto_fields() {
        let hp = HostProto {
            proto: "https",
            host: "example.com",
        };
        assert_eq!(hp.proto, "https");
        assert_eq!(hp.host, "example.com");
    }
}
