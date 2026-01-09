mod base;
mod delete;
mod get;
mod head;
mod options;
mod patch;
mod post;

use std::{collections::{HashMap, HashSet}, ops::{Deref, DerefMut}};

use base64::Engine;
use salvo_core::{http::{HeaderMap, HeaderValue}};
pub use options::options_handler;
pub use post::post_handler;
pub use head::head_handler;
pub use patch::patch_handler;
pub use delete::delete_handler;
pub use get::get_handler;

use crate::{H_TUS_RESUMABLE, TUS_VERSION, error::ProtocolError};

pub(crate) const EXPOSE_HEADERS: &str = "Location, Upload-Offset, Upload-Length, Upload-Metadata, Tus-Resumable, Tus-Version, Tus-Extension, Tus-Max-Size";

pub(crate) fn apply_common_headers(headers: &mut HeaderMap) -> &mut HeaderMap {
    headers.insert(H_TUS_RESUMABLE, HeaderValue::from_static(TUS_VERSION));
    headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    headers.insert("access-control-expose-headers", HeaderValue::from_static(EXPOSE_HEADERS));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));

    headers
}

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

    pub fn stringify(metadata: Metadata) -> String {
        metadata
            .0
            .iter()
            .map(|(key, value)| {
                let encoded = base64::engine::general_purpose::STANDARD.encode(value.as_bytes());
                format!("{} {}", key, encoded)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl Deref for Metadata {
    type Target = HashMap<String, String>;

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
