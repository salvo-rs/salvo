use std::collections::HashMap;

use base64::Engine;

use crate::{TUS_VERSION, error::ProtocolError};

pub fn require_tus_version(v: Option<&str>) -> Result<(), ProtocolError> {
    let v = v.ok_or(ProtocolError::MissingTusResumable)?;
    if v != TUS_VERSION {
        return Err(ProtocolError::UnsupportedTusVersion(v.to_string()));
    }
    Ok(())
}

pub fn parse_u64(v: Option<&str>, name: &'static str) -> Result<u64, ProtocolError> {
    let s = v.ok_or(ProtocolError::MissingHeader(name))?;
    s.parse::<u64>().map_err(|_| ProtocolError::InvalidInt(name))
}

/// Upload-Metadata: "filename dGVzdC5tcDQ=,foo YmFy"
pub fn parse_metadata(v: Option<&str>) -> Result<HashMap<String, String>, ProtocolError> {
    let Some(s) = v else { return Ok(HashMap::new()); };
    if s.trim().is_empty() { return Ok(HashMap::new()); }

    let mut map = HashMap::new();
    for item in s.split(',') {
        let item = item.trim();
        if item.is_empty() { continue; }

        let mut parts = item.splitn(2, ' ');
        let key = parts.next().ok_or(ProtocolError::InvalidMetadata)?.trim();
        let b64 = parts.next().unwrap_or("").trim();

        let val = if b64.is_empty() {
            "".to_string()
        } else {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|_| ProtocolError::InvalidMetadata)?;
            String::from_utf8(bytes).map_err(|_| ProtocolError::InvalidMetadata)?
        };

        map.insert(key.to_string(), val);
    }
    Ok(map)
}

pub fn normalize_path(p: &str) -> String {
    if p.is_empty() {
        return "/".to_string();
    }
    let mut out = p.to_string();
    if !out.starts_with('/') {
        out = format!("/{}", out);
    }
    if out.len() > 1 {
        out = out.trim_end_matches('/').to_string();
    }
    out
}