use std::sync::Arc;

use salvo_core::{Depot, Request, Response, handler};

use crate::{Metadata, Tus, error::{ProtocolError, TusError}, stores::{Extension, UploadInfo}, utils::parse_metadata};

#[handler]
pub async fn create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let store = &state.store;
    let opts = &state.options;

    if req.headers().get("upload-concat").is_some() && !store.has_extension(Extension::Concatentation) {
        res.status_code = Some(TusError::Protocol(ProtocolError::UnsupportedConcatenationExtension).status());
        return;
    }

    let upload_length = req.headers().get("upload-length");
    let upload_defer_length = req.headers().get("upload-defer-length");
    let upload_metadata = req.headers().get("upload-metadata");

    if upload_defer_length.is_some() && !store.has_extension(Extension::CreationDeferLength) {
        res.status_code = Some(TusError::Protocol(ProtocolError::UnsupportedCreationDeferLengthExtension).status());
        return;
    }

    if upload_length.is_none() == upload_defer_length.is_none() {
        res.status_code = Some(TusError::Protocol(ProtocolError::InvalidLength).status());
        return;
    }

    let metadata = match upload_metadata {
        Some(upload_metadata) => {
            let raw = upload_metadata
                .to_str()
                .map_err(|_| ProtocolError::InvalidMetadata);
            match raw {
                Ok(s) => match parse_metadata(Some(s)) {
                    Ok(m) => Metadata(m),
                    Err(e) => {
                        res.status_code = Some(TusError::Protocol(e).status());
                        return;
                    },
                },
                Err(e) => {
                    res.status_code = Some(TusError::Protocol(e).status());
                    return;
                },
            }
        },
        None => {
            res.status_code = Some(TusError::Protocol(ProtocolError::InvalidMetadata).status());
            return;
        },
    };

    let Ok(id) = (opts.naming_function)(req, metadata.clone()) else {
        res.status_code = Some(TusError::GenerateIdError.status());
        return;
    };

    let max_file_size = opts.get_configured_max_size(req, &id).await;

    if upload_length.is_some() &&
        max_file_size > 0 &&
        upload_length.and_then(|hv| hv.to_str().ok()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) > max_file_size
    {
        res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
        return;
    }

    if opts.on_incoming_request.is_some() {
        let fut = (opts.on_incoming_request.as_ref().unwrap())(req, &id, &metadata);
        if let Err(tus_err) = fut.await {
            res.status_code = Some(tus_err.status());
            return;
        }
    }

    let mut upload = UploadInfo::new(id, 0);
    upload.size = upload_length.and_then(|hv| hv.to_str().ok()).and_then(|s| s.parse::<u64>().ok());

    

}
