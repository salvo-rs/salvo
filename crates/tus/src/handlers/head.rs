use std::sync::Arc;

use salvo_core::{Depot, Request, Response, Router, handler, http::{HeaderValue, StatusCode}};

use crate::{Tus, error::TusError, handlers::{Metadata, apply_common_headers}};

#[handler]
async fn head(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let opts = &state.options;
    let store = &state.store;
    let headers = apply_common_headers(&mut res.headers);

    let id = match opts.get_file_id_from_request(req) {
        Ok(id) => id,
        Err(e) => {
            res.status_code(e.status());
            return;
        }
    };

    // TODO: handle _on_incoming_request(req, id);
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
}


pub fn head_handler() -> Router {
    let head_router = Router::with_path("{id}")
        .head(head);
    head_router
}
