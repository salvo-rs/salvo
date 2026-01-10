use std::sync::Arc;

use salvo_core::{Depot, Request, Response, Router, handler, http::{HeaderValue, StatusCode}};

use crate::{
    H_TUS_RESUMABLE, H_TUS_VERSION, H_UPLOAD_EXPIRES, TUS_VERSION, Tus,
    error::{ProtocolError, TusError}, handlers::{Metadata, apply_common_headers},
    stores::Extension, utils::check_tus_version
};

#[handler]
async fn head(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let opts = &state.options;
    let store = &state.store;
    let headers = apply_common_headers(&mut res.headers);

    if let Err(e) = check_tus_version(
        req.headers()
            .get(H_TUS_RESUMABLE)
            .and_then(|v| v.to_str().ok()),
    ) {
        if matches!(e, ProtocolError::UnsupportedTusVersion(_)) {
            headers.insert(H_TUS_VERSION, HeaderValue::from_static(TUS_VERSION));
        }
        res.status_code(TusError::Protocol(e).status());
        return;
    }

    let id = match opts.get_file_id_from_request(req) {
        Ok(id) => id,
        Err(e) => {
            res.status_code(e.status());
            return;
        }
    };

    if let Some(on_incoming_request) = &opts.on_incoming_request {
        on_incoming_request(req, id.clone()).await;
    }
    // TODO: let lock = opts.acquire_lock(req, &id, context);

    let upload_info = match store.get_upload_file_info(&id).await {
        Ok(info) => info,
        Err(e) => {
            // lock.unlock()
            res.status_code(e.status());
            return;
        }
    };

    // If a Client does attempt to resume an upload which has since
    // been removed by the Server, the Server SHOULD respond with the
    // with the 404 Not Found or 410 Gone status. The latter one SHOULD
    // be used if the Server is keeping track of expired uploads.

    // 404: deleted
    // 410: expiration

    // TODO: Time handle

    let mut expires_at = None;
    if store.has_extension(Extension::Expiration) {
        if let Some(expiration) = store.get_expiration() {
            if expiration > std::time::Duration::from_secs(0) && !upload_info.creation_date.is_empty() {
                if let Ok(created_at) = chrono::DateTime::parse_from_rfc3339(&upload_info.creation_date) {
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

    res.status_code = Some(StatusCode::OK);

    let Some(offset) = &upload_info.offset else {
        res.status_code = Some(TusError::Internal("Upload file's offset value not found!".into()).status());
        return;
    };
    headers.insert("Upload-Offset", HeaderValue::from_str(&offset.to_string()).unwrap());

    if upload_info.get_size_is_deferred() {
        headers.insert("Upload-Defer-Length", HeaderValue::from_static("1"));
    } else {
        if let Some(size) = &upload_info.size {
            headers.insert("Upload-Length", HeaderValue::from_str(&size.to_string()).unwrap());
        };
    }

    if let Some(metadata) = upload_info.metadata {
        headers.insert("Upload-Metadata", HeaderValue::from_str(&Metadata::stringify(metadata)).unwrap());
    }

    if let Some(expires_at) = expires_at {
        let is_finished = match (upload_info.offset, upload_info.size) {
            (Some(offset), Some(size)) => offset == size,
            _ => false,
        };
        if !is_finished {
            let expires_value = expires_at.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
            headers.insert(
                H_UPLOAD_EXPIRES,
                HeaderValue::from_str(&expires_value).unwrap(),
            );
        }
    }
}


pub fn head_handler() -> Router {
    let head_router = Router::with_path("{id}")
        .head(head);
    head_router
}
