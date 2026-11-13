use std::sync::Arc;

use futures_util::{StreamExt, stream};
use salvo_core::fs::NamedFile;
use salvo_core::http::body::BytesFrame;
use salvo_core::http::{HeaderValue, Method};
use salvo_core::{Depot, Request, Response, Router, handler};

use crate::error::{ProtocolError, TusError};
use crate::handlers::{apply_common_headers, upload_expiration};
use crate::method_override::TusMethodFilter;
use crate::utils::check_tus_version;
use crate::{CancellationContext, H_TUS_RESUMABLE, H_TUS_VERSION, TUS_VERSION, Tus};

#[handler]
async fn get(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get_typed::<Arc<Tus>>().expect("missing tus state");
    let opts = &state.options;
    let store = &state.store;
    let headers = apply_common_headers(req, opts, &mut res.headers);

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

    // Opening and constructing the representation must happen under the same
    // lock as reading its committed offset. Otherwise completion or DELETE can
    // move/remove the path between the metadata lookup and open.
    let lock = match opts
        .acquire_read_lock(req, &id, CancellationContext::new())
        .await
    {
        Ok(lock) => lock,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    let info = match store.get_upload_file_info(&id).await {
        Ok(info) => info,
        Err(e) => {
            res.status_code(e.status());
            return;
        }
    };

    if upload_expiration(store.as_ref(), &info).is_some_and(|expires| chrono::Utc::now() > expires)
    {
        res.status_code = Some(TusError::FileNoLongerExists.status());
        return;
    }
    let Some(offset) = info.offset else {
        res.status_code = Some(TusError::Internal("upload offset missing".into()).status());
        return;
    };
    let Some(storage) = info.storage else {
        res.status_code = Some(TusError::Internal("upload storage info missing".into()).status());
        return;
    };

    if storage.type_name != "file" {
        res.status_code = Some(
            TusError::Internal(format!("unsupported storage type: {}", storage.type_name)).status(),
        );
        return;
    }

    let file = match NamedFile::builder(storage.path)
        .max_length(offset)
        .build()
        .await
    {
        Ok(file) => file,
        Err(_) => {
            res.status_code = Some(TusError::Internal("upload data unavailable".into()).status());
            return;
        }
    };
    file.send(req.headers(), res).await;
    if res.body.is_stream() {
        // Streaming outlives the handler. Keep the shared lock until EOF or
        // response cancellation, so a writer cannot truncate this snapshot and
        // DELETE/finalization cannot replace its backing file while it is read.
        let body = res.take_body();
        res.stream(stream::unfold(
            (body, lock),
            |(mut body, lock)| async move {
                body.next()
                    .await
                    .map(|frame| (frame.map(BytesFrame), (body, lock)))
            },
        ));
    }
}

pub(crate) fn get_handler() -> Router {
    Router::with_path("{id}")
        .filter(TusMethodFilter(Method::GET))
        .goal(get)
}
