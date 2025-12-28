mod base;
mod delete;
mod get;
mod head;
mod options;
mod patch;
mod post;

use std::collections::{HashMap, HashSet};

use base64::Engine;
pub use options::options_handler;
pub use post::post_handler;

use crate::error::ProtocolError;

#[derive(Clone, Debug, Default)]
pub struct Metadata(pub HashMap<String, String>);

impl Metadata {
    pub fn parse_metadata(raw: &str) -> Result<Metadata, ProtocolError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(ProtocolError::InvalidMetadata);
        }

        let mut map = HashMap::new();
        let mut seen = HashSet::new();

        for item in raw.split(',') {
            let item = item.trim();
            if item.is_empty() {
                return Err(ProtocolError::InvalidMetadata);
            }

            let (key, b64) = match item.split_once(' ') {
                Some((k, v)) => (k.trim(), v.trim()),
                None => (item, ""),
            };

            if key.is_empty() || key.contains(' ') || key.contains(',') {
                return Err(ProtocolError::InvalidMetadata);
            }

            if !seen.insert(key.to_string()) {
                return Err(ProtocolError::InvalidMetadata);
            }

            let decoded = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|_| ProtocolError::InvalidMetadata)?;

            let value = match String::from_utf8(decoded) {
                Ok(s) => s,
                Err(_) => b64.to_string(),
            };

            map.insert(key.to_string(), value);
        }

        Ok(Metadata(map))
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