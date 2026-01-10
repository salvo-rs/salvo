use std::sync::Arc;

use futures_util::StreamExt;
use salvo_core::{Depot, Request, Response, Router, handler, http::{HeaderValue, StatusCode}};

use crate::{
    CancellationContext, CT_OFFSET_OCTET_STREAM, H_CONTENT_LENGTH, H_CONTENT_TYPE,
    H_TUS_RESUMABLE, H_TUS_VERSION, H_UPLOAD_EXPIRES, H_UPLOAD_LENGTH, H_UPLOAD_OFFSET,
    TUS_VERSION, Tus, error::{ProtocolError, TusError}, handlers::apply_common_headers,
    stores::Extension, utils::{check_tus_version, parse_u64}
};

#[handler]
async fn patch(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let opts = &state.options;
    let store = &state.store;
    apply_common_headers(&mut res.headers);

    let id = match opts.get_file_id_from_request(req) {
        Ok(id) => id,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    // 1. Check TUS version.
    if let Err(e) = check_tus_version(
        req.headers()
            .get(H_TUS_RESUMABLE)
            .and_then(|v| v.to_str().ok()),
    ) {
        if matches!(e, ProtocolError::UnsupportedTusVersion(_)) {
            res.headers.insert(H_TUS_VERSION, HeaderValue::from_static(TUS_VERSION));
        }
        res.status_code = Some(TusError::Protocol(e).status());
        return;
    }

    // 2. Check Content Type. The request MUST include a Content-Type header
    let content_type = req.headers().get(H_CONTENT_TYPE).and_then(|v| v.to_str().ok());
    if content_type != Some(CT_OFFSET_OCTET_STREAM) {
        res.status_code = Some(TusError::Protocol(ProtocolError::InvalidContentType).status());
        return;
    }

    // 3. Check Upload-Offset. The request MUST include a Upload-Offset header
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

    if let Some(on_incoming_request) = &opts.on_incoming_request {
        on_incoming_request(req, id.clone()).await;
    }

    let max_file_size = opts.get_configured_max_size(req, Some(id.to_string())).await;
    let _lock = match opts.acquire_write_lock(req, &id, CancellationContext::new()).await {
        Ok(lock) => lock,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    let mut already_uploaded_info = match store.get_upload_file_info(&id).await {
        Ok(info) => info,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    let mut expires_at = None;
    if store.has_extension(Extension::Expiration) {
        if let Some(expiration) = store.get_expiration() {
            if expiration > std::time::Duration::from_secs(0) && !already_uploaded_info.creation_date.is_empty() {
                if let Ok(created_at) = chrono::DateTime::parse_from_rfc3339(&already_uploaded_info.creation_date) {
                    if let Ok(delta) = chrono::Duration::from_std(expiration) {
                        let expires = created_at.with_timezone(&chrono::Utc) + delta;
                        if chrono::Utc::now() > expires {
                            res.status_code = Some(TusError::FileNoLongerExists.status());
                            return;
                        }
                        expires_at = Some(expires);
                    }
                }
            }
        }
    }

    // If a Client does attempt to resume an upload which has since
    // been removed by the Server, the Server SHOULD respond with the
    // with the 404 Not Found or 410 Gone status. The latter one SHOULD
    // be used if the Server is keeping track of expired uploads.

    // 404: deleted
    // 410: expiration

    // TODO: Time handle

    let Some(uploaded_info_offset) = already_uploaded_info.offset else {
        res.status_code = Some(TusError::InvalidOffset.status());
        return;
    };

    if uploaded_info_offset != offset {
        tracing::info!(
            "Incorrect offset - {:?} sent but file is {:?}",
            offset,
            uploaded_info_offset
        );
        res.status_code = Some(TusError::InvalidOffset.status());
        return;
    }

    if let Some(raw_length) = req.headers().get(H_UPLOAD_LENGTH) {
        let size = match raw_length.to_str() {
            Ok(value) => match parse_u64(Some(value), H_UPLOAD_LENGTH) {
                Ok(size) => size,
                Err(e) => {
                    res.status_code = Some(TusError::Protocol(e).status());
                    return;
                }
            },
            Err(_) => {
                res.status_code = Some(TusError::Protocol(ProtocolError::InvalidInt(H_UPLOAD_LENGTH)).status());
                return;
            }
        };

        if !store.has_extension(Extension::CreationDeferLength) {
            res.status_code = Some(TusError::Protocol(ProtocolError::UnsupportedCreationDeferLengthExtension).status());
            return;
        }
        // Return if upload-length is already set.
        if already_uploaded_info.size.is_some() {
            res.status_code = Some(TusError::Protocol(ProtocolError::InvalidLength).status());
            return;
        }

        if size < uploaded_info_offset {
            res.status_code = Some(TusError::Protocol(ProtocolError::InvalidLength).status());
            return;
        }

        if max_file_size > 0 && size > max_file_size {
            res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
            return;
        }

        // Update
        let _ = store.declare_upload_length(&id, size).await;
        already_uploaded_info.size = Some(size);
    }

    let content_length = match req.headers().get(H_CONTENT_LENGTH) {
        Some(value) => match value.to_str() {
            Ok(v) => match parse_u64(Some(v), H_CONTENT_LENGTH) {
                Ok(size) => Some(size),
                Err(e) => {
                    res.status_code = Some(TusError::Protocol(e).status());
                    return;
                }
            },
            Err(_) => {
                res.status_code = Some(TusError::Protocol(ProtocolError::InvalidInt(H_CONTENT_LENGTH)).status());
                return;
            }
        },
        None => None,
    };

    let max_allowed = match (already_uploaded_info.size, max_file_size) {
        (Some(size), max) if max > 0 => Some(size.min(max)),
        (Some(size), _) => Some(size),
        (None, max) if max > 0 => Some(max),
        _ => None,
    };

    if let (Some(incoming), Some(max_allowed)) = (content_length, max_allowed) {
        if offset + incoming > max_allowed {
            res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
            return;
        }
    }

    // let max_body_size = opts.calculate_max_body_size(req, already_uploaded_info, max_file_size).await;
    // let new_offset = store.write(req.body, already_uploaded_info, max_body_size, context);

    let body = req.take_body();
    let stream = body.map(|frame| frame.map(|frame| frame.into_data().unwrap_or_default()));
    let written = match store.write(&id, offset, Box::pin(stream)).await {
        Ok(written) => written,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    let new_offset = offset + written;

    if let Some(expires_at) = expires_at {
        let is_finished = match already_uploaded_info.size {
            Some(size) => new_offset == size,
            None => false,
        };

        if !is_finished {
            let expires_value = expires_at.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
            res.headers.insert(
                H_UPLOAD_EXPIRES,
                HeaderValue::from_str(&expires_value).unwrap(),
            );
        }
    }

    // The Server MUST acknowledge successful PATCH requests with the 204 No Content status.
    // It MUST include the Upload-Offset header containing the new offset.
    // The new offset MUST be the sum of the offset before the PATCH request and the number of bytes received and processed or stored during the current PATCH request.
    res.status_code = Some(StatusCode::NO_CONTENT);
    res.headers
        .insert(H_UPLOAD_OFFSET, HeaderValue::from_str(&new_offset.to_string()).unwrap());
}

pub fn patch_handler() -> Router {
    let patch_router = Router::with_path("{id}")
        .patch(patch);
    patch_router
}
