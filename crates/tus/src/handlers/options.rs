use std::sync::Arc;

use salvo_core::http::{HeaderValue, StatusCode};
use salvo_core::{Depot, Request, Response, Router, handler};

use crate::handlers::{DEFAULT_ALLOW_HEADERS, apply_options_headers, insert_joined_header};
use crate::stores::Extension;
use crate::{
    H_ACCESS_CONTROL_ALLOW_HEADERS, H_ACCESS_CONTROL_ALLOW_METHODS, H_ACCESS_CONTROL_MAX_AGE,
    H_TUS_EXTENSION, H_TUS_MAX_SIZE, H_TUS_VERSION, TUS_VERSION, Tus,
};

#[handler]
/// https://tus.io/protocols/resumable-upload#options
async fn options(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get_typed::<Arc<Tus>>().expect("missing tus state");
    let store = &state.store;
    let opts = &state.options;
    let max_size = opts.get_configured_max_size(req, None).await;
    let headers = apply_options_headers(req, opts, &mut res.headers);

    headers.insert(H_TUS_VERSION, HeaderValue::from_static(TUS_VERSION));
    if let Some(ext_header) = Extension::to_header_value(&store.extensions()) {
        headers.insert(H_TUS_EXTENSION, ext_header);
    }

    if max_size > 0 {
        headers.insert(H_TUS_MAX_SIZE, HeaderValue::from(max_size));
    }

    headers.insert(
        H_ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("OPTIONS, POST, HEAD, PATCH, DELETE, GET"),
    );

    if !opts.allowed_headers.is_empty() {
        insert_joined_header(
            headers,
            H_ACCESS_CONTROL_ALLOW_HEADERS,
            DEFAULT_ALLOW_HEADERS,
            &opts.allowed_headers,
        );
    } else {
        // Advertise a fixed allow-list instead of reflecting the client's
        // `Access-Control-Request-Headers`. Echoing that header back unchanged
        // mirrors arbitrary attacker-chosen header names and, combined with
        // origin reflection, widens the CORS attack surface.
        headers.insert(
            H_ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(DEFAULT_ALLOW_HEADERS),
        );
    }

    headers.insert(H_ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("86400"));
    res.status_code(StatusCode::NO_CONTENT);
}

pub(crate) fn options_handler() -> Router {
    Router::new()
        .options(options)
        .push(Router::with_path("{id}").options(options))
}
