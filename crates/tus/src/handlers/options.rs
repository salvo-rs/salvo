use std::sync::Arc;

use salvo_core::{Depot, Request, Response, Router, handler, http::{HeaderValue, StatusCode}};

use crate::{H_TUS_EXTENSION, H_TUS_VERSION, TUS_VERSION, Tus, handlers::apply_common_headers};

#[handler]
/// https://tus.io/protocols/resumable-upload#options
async fn options(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let _opts = &state.options;

    apply_common_headers(res);

    res.status_code(StatusCode::NO_CONTENT);
    res.headers_mut().insert(H_TUS_VERSION, HeaderValue::from_static(TUS_VERSION));
    res.headers_mut().insert(H_TUS_EXTENSION, HeaderValue::from_static("creation"));
    res.headers_mut().insert("access-control-allow-methods", HeaderValue::from_static("OPTIONS, POST, HEAD, PATCH"));

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
}

pub fn options_handler() -> Router {
    let options_router = Router::new()
            .options(options)
            .push(Router::with_path("{id}").options(options));
    options_router
}
