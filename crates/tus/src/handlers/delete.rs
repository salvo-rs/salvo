use std::sync::Arc;

use salvo_core::{Depot, Request, Response, Router, handler, http::{HeaderValue, StatusCode}};

use crate::{
    H_TUS_RESUMABLE, H_TUS_VERSION, TUS_VERSION, Tus, error::{ProtocolError, TusError},
    handlers::apply_common_headers, stores::Extension, utils::check_tus_version
};

#[handler]
async fn delete(req: &mut Request, depot: &mut Depot, res: &mut Response) {
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

    if !store.has_extension(Extension::Termination) {
        res.status_code = Some(TusError::Protocol(ProtocolError::UnsupportedTerminationExtension).status());
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

    if opts.disable_termination_for_finished_uploads {
        if let Ok(info) = store.get_upload_file_info(&id).await {
            if let (Some(size), Some(offset)) = (info.size, info.offset) {
                if size == offset {
                    res.status_code = Some(StatusCode::FORBIDDEN);
                    return;
                }
            }
        }
    }

    match store.remove(&id).await {
        Ok(_) => res.status_code = Some(StatusCode::NO_CONTENT),
        Err(e) => res.status_code = Some(e.status()),
    }
}

pub fn delete_handler() -> Router {
    Router::with_path("{id}").delete(delete)
}
