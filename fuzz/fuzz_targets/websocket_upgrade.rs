#![no_main]

use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use salvo_core::http::header::{
    CONNECTION, HeaderValue, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL, SEC_WEBSOCKET_VERSION,
    UPGRADE,
};
use salvo_core::http::{Request, Response};
use salvo_extra::websocket::WebSocketUpgrade;

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("websocket fuzz runtime should build")
    })
}

fuzz_target!(|data: &[u8]| {
    let flags = data.first().copied().unwrap_or_default();
    let payload = data.get(1..).unwrap_or_default();
    let parts = salvo_fuzz::split_even(payload, 4);

    let mut req = Request::new();
    if let Ok(uri) = "/ws".parse() {
        *req.uri_mut() = uri;
    }

    let connection_value = if flags & 0b1 != 0 {
        "Upgrade".to_owned()
    } else {
        salvo_fuzz::safe_header_token(parts[0], 32)
    };
    if let Ok(value) = HeaderValue::from_str(&connection_value) {
        req.headers_mut().insert(CONNECTION, value);
    }

    let upgrade_value = if flags & 0b10 != 0 {
        "websocket".to_owned()
    } else {
        salvo_fuzz::safe_header_token(parts[1], 32)
    };
    if let Ok(value) = HeaderValue::from_str(&upgrade_value) {
        req.headers_mut().insert(UPGRADE, value);
    }

    let version_value = if flags & 0b100 != 0 { "13" } else { "12" };
    if let Ok(value) = HeaderValue::from_str(version_value) {
        req.headers_mut().insert(SEC_WEBSOCKET_VERSION, value);
    }

    let key_value = if flags & 0b1000 != 0 {
        "dGhlIHNhbXBsZSBub25jZQ==".to_owned()
    } else {
        salvo_fuzz::safe_header_token(parts[2], 32)
    };
    if let Ok(value) = HeaderValue::from_str(&key_value) {
        req.headers_mut().insert(SEC_WEBSOCKET_KEY, value);
    }

    let requested_protocols = salvo_fuzz::token_list(parts[2], 3, 16);
    if !requested_protocols.is_empty() {
        let header = requested_protocols.join(", ");
        if let Ok(value) = HeaderValue::from_str(&header) {
            req.headers_mut().insert(SEC_WEBSOCKET_PROTOCOL, value);
        }
    }

    let mut upgrade = WebSocketUpgrade::new();
    if flags & 0b1_0000 != 0 {
        upgrade = upgrade.accept_any_protocol();
    } else {
        let supported = salvo_fuzz::token_list(parts[3], 3, 16);
        let refs = supported.iter().map(String::as_str).collect::<Vec<_>>();
        upgrade = upgrade.protocols(&refs);
    }

    let mut res = Response::new();
    let _ = runtime().block_on(upgrade.upgrade(&mut req, &mut res, |_ws| async move {}));
});
