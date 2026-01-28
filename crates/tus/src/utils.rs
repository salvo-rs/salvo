use salvo_core::http::HeaderValue;

use crate::TUS_VERSION;
use crate::error::ProtocolError;

pub fn check_tus_version(v: Option<&str>) -> Result<(), ProtocolError> {
    let v = v.ok_or(ProtocolError::MissingTusResumable)?;
    if v != TUS_VERSION {
        return Err(ProtocolError::UnsupportedTusVersion(v.to_string()));
    }
    Ok(())
}

pub fn parse_u64(v: Option<&str>, name: &'static str) -> Result<u64, ProtocolError> {
    let s = v.ok_or(ProtocolError::MissingHeader(name))?;
    s.parse::<u64>()
        .map_err(|_| ProtocolError::InvalidInt(name))
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

#[cfg(test)]
mod tests {
    use salvo_core::http::HeaderValue;

    use super::*;

    #[test]
    fn test_check_tus_version_valid() {
        assert!(check_tus_version(Some("1.0.0")).is_ok());
    }

    #[test]
    fn test_check_tus_version_missing() {
        let result = check_tus_version(None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProtocolError::MissingTusResumable
        ));
    }

    #[test]
    fn test_check_tus_version_unsupported() {
        let result = check_tus_version(Some("2.0.0"));
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::UnsupportedTusVersion(v) => assert_eq!(v, "2.0.0"),
            _ => panic!("Expected UnsupportedTusVersion error"),
        }
    }

    #[test]
    fn test_check_tus_version_invalid_format() {
        let result = check_tus_version(Some("1.0"));
        assert!(result.is_err());

        let result = check_tus_version(Some(""));
        assert!(result.is_err());

        let result = check_tus_version(Some("abc"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_u64_valid() {
        assert_eq!(parse_u64(Some("0"), "test").unwrap(), 0);
        assert_eq!(parse_u64(Some("123"), "test").unwrap(), 123);
        assert_eq!(
            parse_u64(Some("18446744073709551615"), "test").unwrap(),
            u64::MAX
        );
    }

    #[test]
    fn test_parse_u64_missing() {
        let result = parse_u64(None, "Upload-Length");
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::MissingHeader(name) => assert_eq!(name, "Upload-Length"),
            _ => panic!("Expected MissingHeader error"),
        }
    }

    #[test]
    fn test_parse_u64_invalid() {
        let result = parse_u64(Some("abc"), "Upload-Length");
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::InvalidInt(name) => assert_eq!(name, "Upload-Length"),
            _ => panic!("Expected InvalidInt error"),
        }

        // Negative numbers
        let result = parse_u64(Some("-1"), "test");
        assert!(result.is_err());

        // Floating point
        let result = parse_u64(Some("1.5"), "test");
        assert!(result.is_err());

        // Empty string
        let result = parse_u64(Some(""), "test");
        assert!(result.is_err());

        // Overflow
        let result = parse_u64(Some("18446744073709551616"), "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_path_empty() {
        assert_eq!(normalize_path(""), "/");
    }

    #[test]
    fn test_normalize_path_root() {
        assert_eq!(normalize_path("/"), "/");
    }

    #[test]
    fn test_normalize_path_adds_leading_slash() {
        assert_eq!(normalize_path("uploads"), "/uploads");
        assert_eq!(normalize_path("api/tus"), "/api/tus");
    }

    #[test]
    fn test_normalize_path_removes_trailing_slash() {
        assert_eq!(normalize_path("/uploads/"), "/uploads");
        assert_eq!(normalize_path("/api/tus/"), "/api/tus");
        assert_eq!(normalize_path("uploads/"), "/uploads");
    }

    #[test]
    fn test_normalize_path_multiple_trailing_slashes() {
        assert_eq!(normalize_path("/uploads///"), "/uploads");
    }

    #[test]
    fn test_normalize_path_already_normalized() {
        assert_eq!(normalize_path("/uploads"), "/uploads");
        assert_eq!(normalize_path("/api/v1/tus"), "/api/v1/tus");
    }

    #[test]
    fn test_normalize_path_complex() {
        assert_eq!(normalize_path("api/v1/uploads/"), "/api/v1/uploads");
        assert_eq!(normalize_path("/a/b/c/d"), "/a/b/c/d");
    }

    #[test]
    fn test_validate_header_valid() {
        let header = HeaderValue::from_static("application/offset+octet-stream");
        assert!(validate_header(
            "application/offset+octet-stream",
            Some(&header)
        ));
    }

    #[test]
    fn test_validate_header_case_insensitive() {
        let header = HeaderValue::from_static("APPLICATION/OFFSET+OCTET-STREAM");
        assert!(validate_header(
            "application/offset+octet-stream",
            Some(&header)
        ));

        let header = HeaderValue::from_static("Application/Offset+Octet-Stream");
        assert!(validate_header(
            "application/offset+octet-stream",
            Some(&header)
        ));
    }

    #[test]
    fn test_validate_header_with_whitespace() {
        let header = HeaderValue::from_static("  application/offset+octet-stream  ");
        assert!(validate_header(
            "application/offset+octet-stream",
            Some(&header)
        ));
    }

    #[test]
    fn test_validate_header_none() {
        assert!(!validate_header("application/offset+octet-stream", None));
    }

    #[test]
    fn test_validate_header_mismatch() {
        let header = HeaderValue::from_static("text/plain");
        assert!(!validate_header("application/json", Some(&header)));
    }

    #[test]
    fn test_validate_header_empty() {
        let header = HeaderValue::from_static("");
        assert!(!validate_header("application/json", Some(&header)));
    }
}
