use std::sync::Arc;

use futures_util::StreamExt;
use salvo_core::{Depot, Request, Response, Router, handler, http::{HeaderValue, StatusCode}};

use crate::{
    CT_OFFSET_OCTET_STREAM, H_CONTENT_TYPE, H_TUS_RESUMABLE, H_UPLOAD_OFFSET, Tus,
    error::{ProtocolError, TusError},
    handlers::apply_common_headers,
    utils::{parse_u64, require_tus_version},
};

#[handler]
async fn patch(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let opts = &state.options;
    let store = &state.store;

    apply_common_headers(res);

    if let Err(e) = require_tus_version(
        req.headers()
            .get(H_TUS_RESUMABLE)
            .and_then(|v| v.to_str().ok()),
    ) {
        res.status_code = Some(TusError::Protocol(e).status());
        return;
    }

    let content_type = req.headers().get(H_CONTENT_TYPE).and_then(|v| v.to_str().ok());
    if content_type != Some(CT_OFFSET_OCTET_STREAM) {
        res.status_code = Some(TusError::Protocol(ProtocolError::InvalidContentType).status());
        return;
    }

    let offset = match parse_u64(
        req.headers().get(H_UPLOAD_OFFSET).and_then(|v| v.to_str().ok()),
        H_UPLOAD_OFFSET,
    ) {
        Ok(offset) => offset,
        Err(e) => {
            res.status_code = Some(TusError::Protocol(e).status());
            return;
        }
    };

    let id = match opts.get_file_id_from_request(req) {
        Ok(id) => id,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    let body = req.take_body();
    let stream = body.map(|frame| frame.map(|frame| frame.into_data().unwrap_or_default()));
    let written = match store.write(&id, offset, Box::pin(stream)).await {
        Ok(written) => written,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    res.status_code = Some(StatusCode::NO_CONTENT);
    res.headers_mut()
        .insert(H_UPLOAD_OFFSET, HeaderValue::from_str(&(offset + written).to_string()).unwrap());
}

pub fn patch_handler() -> Router {
    let patch_router = Router::with_path("{id}")
        .patch(patch);
    patch_router
}
