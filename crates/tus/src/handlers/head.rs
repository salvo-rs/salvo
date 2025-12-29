use std::sync::Arc;

use salvo_core::{Depot, Request, Response, Router, handler, http::{HeaderValue, StatusCode}};

use crate::{Tus, handlers::{Metadata, apply_common_headers}};

#[handler]
async fn head(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let opts = &state.options;
    let store = &state.store;

    apply_common_headers(res);

    let id = opts.get_file_id_from_request(req);
    println!("id: {:?}", id);
    match id {
        Ok(id) => {
            // handle _on_incoming_request
            // let lock = opts.acquire_lock(req, &id, context);

            let file = store.get_upload(&id).await;
            match file {
                Ok(file) => {
                    // 1. If a Client does attempt to resume an upload which has since
                    // been removed by the Server, the Server SHOULD respond with the
                    // with the 404 Not Found or 410 Gone status. The latter one SHOULD
                    // be used if the Server is keeping track of expired uploads.

                    res.status_code = Some(StatusCode::OK);
                    res.headers_mut().insert("Upload-Offset", HeaderValue::from_str(&file.offset.unwrap().to_string()).unwrap());

                    if file.get_size_is_deferred() {
                        res.headers_mut().insert("Upload-Defer-Length", HeaderValue::from_static("1"));
                    } else {
                        res.headers_mut().insert("Upload-Length", HeaderValue::from_str(&file.size.unwrap().to_string()).unwrap());
                    }

                    if file.metadata.is_some() {
                        res.headers_mut().insert("Upload-Metadata", HeaderValue::from_str(&Metadata::stringify(file.metadata.unwrap())).unwrap());
                    }
                }
                Err(e) => {
                    // lock.unlock()
                    res.status_code = Some(e.status());
                    return ;
                }
            }
        }
        Err(e) => {
            res.status_code = Some(e.status());
            return ;
        }
    }
}


pub fn head_handler() -> Router {
    let head_router = Router::with_path("{id}")
            .head(head);
    head_router
}
