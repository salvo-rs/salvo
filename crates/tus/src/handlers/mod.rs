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
