#![no_main]

use libfuzzer_sys::fuzz_target;
use salvo_core::http::Request;
use salvo_core::http::header::{HOST, HeaderValue};
use salvo_tus::options::TusOptions;

fuzz_target!(|data: &[u8]| {
    let flags = data.first().copied().unwrap_or_default();
    let payload = data.get(1..).unwrap_or_default();
    let parts = salvo_fuzz::split_even(payload, 6);

    let request_path = salvo_fuzz::safe_uri_path(parts[0], 128);
    let option_path = salvo_fuzz::safe_route_path(parts[1], 64);
    let upload_id = salvo_fuzz::non_empty_token(parts[2], "upload", 64);
    let forwarded_host = salvo_fuzz::safe_host(parts[3], 48);
    let host = salvo_fuzz::non_empty_token(parts[4], "localhost", 48);
    let requested_proto = if flags & 0b100 == 0 { "https" } else { "http" };

    let mut req = Request::new();
    if let Ok(uri) = request_path.parse() {
        *req.uri_mut() = uri;
    }

    if let Ok(value) = HeaderValue::from_str(&host) {
        req.headers_mut().insert(HOST, value);
    }

    if !forwarded_host.is_empty() {
        let forwarded = format!("for=192.0.2.1; proto={requested_proto}; host={forwarded_host}");
        if let Ok(value) = HeaderValue::from_str(&forwarded) {
            req.headers_mut().insert("forwarded", value);
        }
    }

    let x_forwarded_host = salvo_fuzz::safe_host(parts[5], 48);
    if !x_forwarded_host.is_empty() {
        if let Ok(value) = HeaderValue::from_str(&x_forwarded_host) {
            req.headers_mut().insert("x-forwarded-host", value);
        }
        if let Ok(value) = HeaderValue::from_str(requested_proto) {
            req.headers_mut().insert("x-forwarded-proto", value);
        }
    }

    if let Ok(value) = HeaderValue::from_str(if flags & 0b1000 == 0 { "on" } else { "off" }) {
        req.headers_mut().insert("x-forwarded-ssl", value);
    }

    let options = TusOptions {
        path: option_path,
        relative_location: flags & 0b1_0000 != 0,
        respect_forwarded_headers: flags & 0b10 != 0,
        ..TusOptions::default()
    };

    let _ = options.get_file_id_from_request(&req);
    let _ = options.generate_upload_url(&mut req, &upload_id);
});
