use salvo_core::http::HeaderValue;

use crate::{TUS_VERSION, error::ProtocolError};

pub fn check_tus_version(v: Option<&str>) -> Result<(), ProtocolError> {
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

pub fn validate_header(name: &'static str, value: Option<&HeaderValue>) -> bool {
    match value {
        Some(v) => {
            if let Ok(s) = v.to_str() {
                s.trim().eq_ignore_ascii_case(name)
            } else {
                false
            }
        }
        None => false,
    }
}