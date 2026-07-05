use std::sync::Arc;

use salvo_core::http::{HeaderValue, StatusCode};
use salvo_core::{Depot, Request, Response, Router, handler};

use crate::error::{ProtocolError, TusError};
use crate::handlers::{Metadata, apply_common_headers};
use crate::stores::Extension;
use crate::utils::check_tus_version;
use crate::{
    CancellationContext, H_TUS_RESUMABLE, H_TUS_VERSION, H_UPLOAD_EXPIRES, TUS_VERSION, Tus,
};

#[handler]
async fn head(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get_typed::<Arc<Tus>>().expect("missing tus state");
    let opts = &state.options;
    let store = &state.store;
    apply_common_headers(req, opts, res.headers_mut());

    if let Err(e) = check_tus_version(
        req.headers()
            .get(H_TUS_RESUMABLE)
            .and_then(|v| v.to_str().ok()),
    ) {
        if matches!(e, ProtocolError::UnsupportedTusVersion(_)) {
            res.headers_mut()
                .insert(H_TUS_VERSION, HeaderValue::from_static(TUS_VERSION));
        }
        res.status_code(TusError::Protocol(e).status());
        return;
    }

    let id = match opts.extract_file_id_from_request(req) {
        Ok(id) => id,
        Err(e) => {
            res.status_code(e.status());
            return;
        }
    };

    if let Some(on_incoming_request) = &opts.on_incoming_request {
        on_incoming_request(req, id.clone()).await;
    }
    let upload_info = {
        let _lock = match opts
            .acquire_read_lock(req, &id, CancellationContext::new())
            .await
        {
            Ok(lock) => lock,
            Err(e) => {
                res.status_code(e.status());
                return;
            }
        };

        match store.get_upload_file_info(&id).await {
            Ok(info) => info,
            Err(e) => {
                res.status_code(e.status());
                return;
            }
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
    if store.has_extension(Extension::Expiration)
        && let Some(expiration) = store.get_expiration()
        && expiration > std::time::Duration::from_secs(0)
        && !upload_info.creation_date.is_empty()
        && let Ok(created_at) = chrono::DateTime::parse_from_rfc3339(&upload_info.creation_date)
        && let Ok(delta) = chrono::Duration::from_std(expiration)
    {
        let expires = created_at.with_timezone(&chrono::Utc) + delta;
        if chrono::Utc::now() > expires {
            res.status_code(TusError::FileNoLongerExists.status());
            return;
        }
        expires_at = Some(expires);
    }

    res.status_code(StatusCode::OK);

    let Some(offset) = &upload_info.offset else {
        res.status_code(
            TusError::Internal("Upload file's offset value not found!".into()).status(),
        );
        return;
    };
    res.headers_mut()
        .insert("Upload-Offset", HeaderValue::from(*offset));

    if upload_info.is_size_deferred() {
        res.headers_mut()
            .insert("Upload-Defer-Length", HeaderValue::from_static("1"));
    } else if let Some(size) = &upload_info.size {
        res.headers_mut()
            .insert("Upload-Length", HeaderValue::from(*size));
    }

    if let Some(metadata) = upload_info.metadata {
        if let Ok(v) = HeaderValue::from_str(&Metadata::stringify(metadata)) {
            res.headers_mut().insert("Upload-Metadata", v);
        } else {
            res.status_code(
                TusError::Internal("Stored Upload-Metadata is not a valid header".into()).status(),
            );
            return;
        }
    }

    if let Some(expires_at) = expires_at {
        let is_finished = match (upload_info.offset, upload_info.size) {
            (Some(offset), Some(size)) => offset == size,
            _ => false,
        };
        if !is_finished {
            let expires_value = expires_at.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
            if let Ok(v) = HeaderValue::from_str(&expires_value) {
                res.headers_mut().insert(H_UPLOAD_EXPIRES, v);
            }
        }
    }
}

pub(crate) fn head_handler() -> Router {
    Router::with_path("{id}").head(head)
}
