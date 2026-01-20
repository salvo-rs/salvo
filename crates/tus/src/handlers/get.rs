use std::sync::Arc;

use salvo_core::http::HeaderValue;
use salvo_core::{Depot, Request, Response, Router, handler};

use crate::error::{ProtocolError, TusError};
use crate::handlers::apply_common_headers;
use crate::utils::check_tus_version;
use crate::{CancellationContext, H_TUS_RESUMABLE, H_TUS_VERSION, TUS_VERSION, Tus};

#[handler]
async fn get(req: &mut Request, depot: &mut Depot, res: &mut Response) {
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

    let storage = {
        let _lock = match opts
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

        let storage = match info.storage {
            Some(storage) => storage,
            None => {
                res.status_code =
                    Some(TusError::Internal("upload storage info missing".into()).status());
                return;
            }
        };

        if storage.type_name != "file" {
            res.status_code = Some(
                TusError::Internal(format!("unsupported storage type: {}", storage.type_name))
                    .status(),
            );
            return;
        }

        storage
    };

    res.send_file(storage.path, req.headers()).await;
}

pub fn get_handler() -> Router {
    Router::with_path("{id}").get(get)
}
