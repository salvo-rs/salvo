use std::sync::Arc;

use salvo_core::{Depot, Request, Response, handler, http::{HeaderValue, StatusCode}};

use crate::{H_TUS_EXTENSION, H_TUS_RESUMABLE, H_TUS_VERSION, TUS_VERSION, Tus};

#[handler]
pub async fn options_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let _opts = &state.options;

    res.status_code(StatusCode::NO_CONTENT);
    res.headers_mut().insert(H_TUS_VERSION, HeaderValue::from_static(TUS_VERSION));
    res.headers_mut().insert(H_TUS_EXTENSION, HeaderValue::from_static("creation"));
    res.headers_mut().insert(H_TUS_RESUMABLE, HeaderValue::from_static(TUS_VERSION));

    // if let Some(ms) = state.max_size {
    //     if let Ok(v) = HeaderValue::from_str(&ms.to_string()) {
    //         res.headers_mut().insert(H_TUS_MAX_SIZE, v);
    //     }
    // }

    res.headers_mut().insert("access-control-allow-origin", HeaderValue::from_static("*"));

    // Optional: allow credentials if you implement auth via cookies.
    // Note: if you set allow-credentials=true, allow-origin MUST NOT be "*".
    // res.headers_mut().insert(
    //     "access-control-allow-credentials",
    //     HeaderValue::from_static("true"),
    // );

    res.headers_mut().insert("access-control-allow-methods", HeaderValue::from_static("OPTIONS, POST, HEAD, PATCH"));

    // If the browser sent Access-Control-Request-Headers, echo them back
    // for maximum compatibility (prevents header mismatch failures).
    if let Some(h) = req
        .headers()
        .get("access-control-request-headers")
        .and_then(|v| v.to_str().ok())
    {
        if let Ok(v) = HeaderValue::from_str(h) {
            res.headers_mut()
                .insert("access-control-allow-headers", v);
        }
    } else {
        // fallback allow list
        res.headers_mut().insert(
            "access-control-allow-headers",
            HeaderValue::from_static(
                "Tus-Resumable, Upload-Length, Upload-Offset, Upload-Metadata, Content-Type, Content-Length",
            ),
        );
    }

    res.headers_mut().insert("access-control-max-age", HeaderValue::from_static("86400"));

    // Expose headers that tus clients need to read from JS
    res.headers_mut().insert("access-control-expose-headers",
        HeaderValue::from_static(
            "Location, Upload-Offset, Upload-Length, Upload-Metadata, Tus-Resumable, Tus-Version, Tus-Extension, Tus-Max-Size",
        ),
    );

    // Optional but recommended: prevent caching
    res.headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
}