#![no_main]

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use libfuzzer_sys::fuzz_target;
use salvo_core::http::Request;
use salvo_core::http::header::{AUTHORIZATION, HeaderValue, PROXY_AUTHORIZATION};
use salvo_extra::basic_auth::parse_credentials;

fuzz_target!(|data: &[u8]| {
    let mode = data.first().copied().unwrap_or_default();
    let payload = &data[1..];

    let header_name = if mode & 1 == 0 {
        AUTHORIZATION
    } else {
        PROXY_AUTHORIZATION
    };

    let header_value = match (mode >> 1) & 0b11 {
        0 => format!("Basic {}", STANDARD.encode(payload)),
        1 => format!("Basic {}", salvo_fuzz::safe_header_token(payload, 256)),
        2 => format!("Bearer {}", salvo_fuzz::safe_header_token(payload, 256)),
        _ => salvo_fuzz::safe_header_token(payload, 256),
    };

    let mut req = Request::new();
    if let Ok(value) = HeaderValue::from_str(&header_value) {
        req.headers_mut().insert(header_name, value);
    }

    let _ = parse_credentials(&req, &[AUTHORIZATION, PROXY_AUTHORIZATION]);
});
