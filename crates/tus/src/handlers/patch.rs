use std::sync::Arc;

use futures_util::StreamExt;
use salvo_core::{Depot, Request, Response, Router, handler, http::{HeaderValue, StatusCode}};

use crate::{
    CT_OFFSET_OCTET_STREAM, H_CONTENT_TYPE, H_TUS_RESUMABLE, H_UPLOAD_LENGTH, H_UPLOAD_OFFSET, Tus, error::{ProtocolError, TusError}, handlers::apply_common_headers, stores::Extension, utils::{check_tus_version, parse_u64}
};

#[handler]
async fn patch(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let opts = &state.options;
    let store = &state.store;
    let headers = apply_common_headers(&mut res.headers);

    let id = match opts.get_file_id_from_request(req) {
        Ok(id) => id,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    // 1. Check TUS version.
    if let Err(e) = check_tus_version(
        headers
            .get(H_TUS_RESUMABLE)
            .and_then(|v| v.to_str().ok()),
    ) {
        res.status_code = Some(TusError::Protocol(e).status());
        return;
    }

    // 2. Check Content Type. The request MUST include a Content-Type header
    let content_type = headers.get(H_CONTENT_TYPE).and_then(|v| v.to_str().ok());
    if content_type != Some(CT_OFFSET_OCTET_STREAM) {
        res.status_code = Some(TusError::Protocol(ProtocolError::InvalidContentType).status());
        return;
    }

    // 3. Check Upload-Offset. The request MUST include a Upload-Offset header
    let offset = match parse_u64(
        headers.get(H_UPLOAD_OFFSET).and_then(|v| v.to_str().ok()),
        H_UPLOAD_OFFSET,
    ) {
        Ok(offset) => offset,
        Err(e) => {
            res.status_code = Some(TusError::Protocol(e).status());
            return;
        }
    };

    // TODO: handle _on_incoming_request(req, id);

    let max_file_size = opts.get_configured_max_size(req, Some(id.to_string())).await;
    // TODO: let lock = opts.acquire_lock(req, &id, context);

    let mut upload_info = match store.get_upload_file_info(&id).await {
        Ok(info) => info,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    // If a Client does attempt to resume an upload which has since
    // been removed by the Server, the Server SHOULD respond with the
    // with the 404 Not Found or 410 Gone status. The latter one SHOULD
    // be used if the Server is keeping track of expired uploads.

    // 404: deleted
    // 410: experiation

    // TODO: Time handle

    let Some(upload_info_offset) = upload_info.offset else {
        res.status_code = Some(TusError::InvalidOffset.status());
        return;
    };

    if upload_info_offset != offset {
        tracing::info!(
            "Incorrect offset - {:?} sent but file is {:?}",
            offset,
            upload_info_offset
        );
        res.status_code = Some(TusError::InvalidOffset.status());
        return;
    }

    // The request MUST validate upload-length related headers
    match parse_u64(
        headers.get(H_UPLOAD_LENGTH).and_then(|v| v.to_str().ok()),
        H_UPLOAD_LENGTH
    ) {
        Ok(size) => {
            if !store.has_extension(Extension::CreationDeferLength) {
                res.status_code = Some(TusError::Protocol(ProtocolError::UnsupportedCreationDeferLengthExtension).status());
                return;
            }
            // Return if upload-length is already set.
            if upload_info.size.is_some() {
                res.status_code = Some(TusError::Protocol(ProtocolError::InvalidLength).status());
                return;
            }

            if size < upload_info_offset {
                res.status_code = Some(TusError::Protocol(ProtocolError::InvalidLength).status());
                return;
            }

            if max_file_size > 0 && size > max_file_size {
                res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
                return;
            }

            // Update
            let _ = store.declare_upload_length(&id, size).await;
            upload_info.size = Some(size);
        },
        Err(e) => {
            res.status_code = Some(TusError::Protocol(e).status());
            return;
        }
    };

    // let max_body_size = opts.calculate_max_body_size(req, upload_info, max_file_size).await;
    // let new_offset = store.write(req.body, upload_info, max_body_size, context);

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
    headers
        .insert(H_UPLOAD_OFFSET, HeaderValue::from_str(&(offset + written).to_string()).unwrap());
}

pub fn patch_handler() -> Router {
    let patch_router = Router::with_path("{id}")
        .patch(patch);
    patch_router
}
