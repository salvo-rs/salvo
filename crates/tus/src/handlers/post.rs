use std::sync::Arc;

use salvo_core::{Depot, Request, Response, Router, handler, http::{HeaderValue, StatusCode}};

use crate::{
    Tus,
    error::{ProtocolError, TusError},
    handlers::{Metadata, apply_common_headers},
    stores::{Extension, UploadInfo}
};

/// HTTP/1.1 201 Created
/// Location: https://tus.example.org/files/24e533e02ec3bc40c387f1a0e460e216
/// Tus-Resumable: 1.0.0
#[handler]
async fn create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let store = &state.store;
    let opts = &state.options;

    apply_common_headers(res);

    let upload_length = req.headers().get("upload-length");
    let upload_defer_length = req.headers().get("upload-defer-length");
    let upload_metadata = req.headers().get("upload-metadata");

    if upload_defer_length.is_some() && !store.has_extension(Extension::CreationDeferLength) {
        res.status_code = Some(TusError::Protocol(ProtocolError::UnsupportedCreationDeferLengthExtension).status());
        return;
    }

    // Must provide either Upload-Length or Upload-Defer-Length, but not both or neither
    if upload_length.is_none() == upload_defer_length.is_none() {
        res.status_code = Some(TusError::Protocol(ProtocolError::InvalidLength).status());
        return;
    }

    // Retrieve and parse metadata
    let metadata = match upload_metadata
        .ok_or(ProtocolError::InvalidMetadata)
        .and_then(|v| v.to_str().map_err(|_| ProtocolError::InvalidMetadata))
        .and_then(Metadata::parse_metadata)
    {
        Ok(m) => m,
        Err(e) => {
            res.status_code = Some(TusError::Protocol(e).status());
            return;
        }
    };

    let upload_id = match (opts.upload_id_naming_function)(req, metadata.clone()) {
        Ok(id) => id,
        Err(err) => {
            res.status_code = Some(err.status());
            return;
        }
    };


    println!("Generated upload ID: {}", &upload_id);

    let max_file_size = opts.get_configured_max_size(req, &upload_id).await;

    if upload_length.is_some() &&
        max_file_size > 0 &&
        upload_length.and_then(|hv| hv.to_str().ok()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) > max_file_size
    {
        res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
        return;
    }


    let mut upload = UploadInfo::new(upload_id.clone());
    upload.metadata = Some(metadata);
    upload.size = upload_length.and_then(|hv| hv.to_str().ok()).and_then(|s| s.parse::<u64>().ok());
    upload.offset = Some(0);

    res.status_code = Some(StatusCode::CREATED);


    let _ = store.create(upload.clone()).await;
    let url = match state.generate_upload_url(req, &upload_id) {
        Ok(url) => url,
        Err(_) => {
            res.status_code = Some(TusError::GenerateUploadURLError.status());
            return ;
        }
    };

    println!("generate url: {}", url);

    // let is_final = upload.size == Some(0) && !upload.get_size_is_deferred();

    if res.status_code == Some(StatusCode::CREATED) || res.status_code.unwrap().is_redirection() {
        res.headers.insert("Location", HeaderValue::from_str(&url).unwrap());
    }

}

pub fn post_handler() -> Router {
    let post_router = Router::new()
        .post(create);
    post_router
}
