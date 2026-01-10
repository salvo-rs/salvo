use std::sync::Arc;

use futures_util::StreamExt;
use salvo_core::{Depot, Request, Response, Router, handler, http::{HeaderValue, StatusCode}};

use crate::{
    CT_OFFSET_OCTET_STREAM, H_CONTENT_LENGTH, H_CONTENT_TYPE, H_TUS_RESUMABLE, H_TUS_VERSION,
    H_UPLOAD_CONCAT, H_UPLOAD_DEFER_LENGTH, H_UPLOAD_EXPIRES, H_UPLOAD_LENGTH, H_UPLOAD_METADATA,
    H_UPLOAD_OFFSET, TUS_VERSION, Tus, error::{ProtocolError, TusError},
    handlers::{Metadata, apply_common_headers}, stores::{Extension, UploadInfo},
    utils::{check_tus_version, parse_u64}
};

/// HTTP/1.1 201 Created
/// Location: https://tus.example.org/files/24e533e02ec3bc40c387f1a0e460e216
/// Tus-Resumable: 1.0.0
#[handler]
async fn create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<Tus>>().expect("missing tus state");
    let store = &state.store;
    let opts = &state.options;
    apply_common_headers(&mut res.headers);
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

    if req.headers().get(H_UPLOAD_CONCAT).is_some() && !store.has_extension(Extension::Concatenation) {
        res.status_code = Some(TusError::Protocol(ProtocolError::UnsupportedConcatenationExtension).status());
        return;
    }

    let upload_length = req.headers().get(H_UPLOAD_LENGTH);
    let upload_defer_length = req.headers().get(H_UPLOAD_DEFER_LENGTH);
    let upload_metadata = req.headers().get(H_UPLOAD_METADATA);

    if upload_defer_length.is_some() && !store.has_extension(Extension::CreationDeferLength) {
        res.status_code = Some(TusError::Protocol(ProtocolError::UnsupportedCreationDeferLengthExtension).status());
        return;
    }

    if let Some(value) = upload_defer_length {
        match value.to_str() {
            Ok(v) if v == "1" => {}
            _ => {
                res.status_code = Some(TusError::Protocol(ProtocolError::InvalidLength).status());
                return;
            }
        }
    }

    // Must provide either Upload-Length or Upload-Defer-Length, but not both or neither
    if upload_length.is_none() == upload_defer_length.is_none() {
        res.status_code = Some(TusError::Protocol(ProtocolError::InvalidLength).status());
        return;
    }

    let creation_with_upload = match req.headers().get(H_CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
        Some(value) if value == CT_OFFSET_OCTET_STREAM => true,
        Some(_) => {
            res.status_code = Some(TusError::Protocol(ProtocolError::InvalidContentType).status());
            return;
        }
        None => false,
    };
    if creation_with_upload && !store.has_extension(Extension::CreationWithUpload) {
        res.status_code = Some(TusError::Protocol(ProtocolError::UnsupportedCreationWithUploadExtension).status());
        return;
    }

    // Retrieve and parse metadata
    let metadata = match upload_metadata
        .map(|v| {
            v.to_str()
                .map_err(|_| ProtocolError::InvalidMetadata)
                .and_then(Metadata::parse_metadata)
        })
        .transpose()
    {
        Ok(Some(m)) => Some(m),
        Ok(None) => None,
        Err(e) => {
            res.status_code = Some(TusError::Protocol(e).status());
            return;
        }
    };

    let upload_id = match (opts.upload_id_naming_function)(req, metadata.clone()).await {
        Ok(id) => id,
        Err(err) => {
            res.status_code = Some(err.status());
            return;
        }
    };

    let upload_length_value = match upload_length {
        Some(value) => match value.to_str() {
            Ok(v) => match parse_u64(Some(v), H_UPLOAD_LENGTH) {
                Ok(size) => Some(size),
                Err(e) => {
                    res.status_code = Some(TusError::Protocol(e).status());
                    return;
                }
            },
            Err(_) => {
                res.status_code = Some(TusError::Protocol(ProtocolError::InvalidInt(H_UPLOAD_LENGTH)).status());
                return;
            }
        },
        None => None,
    };

    let max_file_size = opts.get_configured_max_size(req, Some(upload_id.to_string())).await;

    if let Some(size) = upload_length_value {
        if max_file_size > 0 && size > max_file_size {
            res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
            return;
        }
    }

    if let Some(on_incoming_request) = &opts.on_incoming_request {
        on_incoming_request(req, upload_id.clone()).await;
    }

    let mut upload = UploadInfo {
        id: upload_id.clone(),
        size: upload_length_value,
        offset: Some(0),
        metadata,
        storage: None,
        creation_date: chrono::Utc::now().to_rfc3339(),
    };

    if let Some(on_upload_create) = &opts.on_upload_create {
        match on_upload_create(req, upload.clone()).await {
            Ok(patch) => {
                if let Some(metadata) = patch.metadata {
                    upload.metadata = Some(metadata);
                }
            }
            Err(e) => {
                res.status_code = Some(e.status());
                return;
            }
        }
    }

    res.status_code = Some(StatusCode::CREATED);

    if let Err(e) = store.create(upload.clone()).await {
        res.status_code = Some(e.status());
        return;
    };

    let url = match opts.generate_upload_url(req, &upload_id) {
        Ok(url) => url,
        Err(_) => {
            res.status_code = Some(TusError::GenerateUploadURLError.status());
            return ;
        }
    };

    tracing::info!("Generated file url: {}", &url);

    if creation_with_upload {
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

        let max_allowed = match (upload.size, max_file_size) {
            (Some(size), max) if max > 0 => Some(size.min(max)),
            (Some(size), _) => Some(size),
            (None, max) if max > 0 => Some(max),
            _ => None,
        };

        if let (Some(incoming), Some(max_allowed)) = (content_length, max_allowed) {
            if incoming > max_allowed {
                res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
                return;
            }
        }

        let body = req.take_body();
        let stream = body.map(|frame| frame.map(|frame| frame.into_data().unwrap_or_default()));
        let written = match store.write(&upload_id, 0, Box::pin(stream)).await {
            Ok(written) => written,
            Err(e) => {
                res.status_code = Some(e.status());
                return;
            }
        };

        upload.offset = Some(written);
        res.headers
            .insert(H_UPLOAD_OFFSET, HeaderValue::from_str(&written.to_string()).unwrap());
    }

    if store.has_extension(Extension::Expiration) {
        if let Some(expiration) = store.get_expiration() {
            if expiration > std::time::Duration::from_secs(0) && !upload.creation_date.is_empty() {
                let created_info = match store.get_upload_file_info(&upload_id).await {
                    Ok(info) => info,
                    Err(e) => {
                        res.status_code = Some(e.status());
                        return;
                    }
                };

                let is_finished = match (created_info.offset, upload.size) {
                    (Some(offset), Some(size)) => offset == size,
                    _ => false,
                };

                if !is_finished {
                    if let Ok(created_at) = chrono::DateTime::parse_from_rfc3339(&upload.creation_date) {
                        if let Ok(delta) = chrono::Duration::from_std(expiration) {
                            let expires = created_at.with_timezone(&chrono::Utc) + delta;
                            let expires_value = expires.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
                            res.headers.insert(
                                H_UPLOAD_EXPIRES,
                                HeaderValue::from_str(&expires_value).unwrap(),
                            );
                        }
                    }
                }
            }
        }
    }

    let is_final = upload.size.is_some_and(|x| x == 0) && !upload.get_size_is_deferred()
        || creation_with_upload && upload.size.is_some_and(|x| x == upload.offset.unwrap_or(0));

    if is_final {
        if let Some(on_upload_finish) = &opts.on_upload_finish {
            match on_upload_finish(req, upload.clone()).await {
                Ok(patch) => {
                    if let Some(status) = patch.status_code {
                        res.status_code = Some(status);
                    }
                    if let Some(body) = patch.body {
                        if res.write_body(body).is_err() {
                            res.status_code = Some(TusError::Internal("failed to write response body".into()).status());
                            return;
                        }
                    }
                    if let Some(headers) = patch.headers {
                        for (key, value) in headers {
                            if let Some(key) = key {
                                if !res.headers.contains_key(&key) {
                                    res.headers.insert(key, value);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    res.status_code = Some(e.status());
                    return;
                }
            }
        }
    }

    // The Upload-Expires response header indicates the time after which the unfinished upload expires.
    // If expiration is known at creation time, Upload-Expires header MUST be included in the response

    if res.status_code == Some(StatusCode::CREATED) || res.status_code.unwrap().is_redirection() {
        res.headers.insert("Location", HeaderValue::from_str(&url).unwrap());
    }

    if res.body.is_none() {
        let status = res.status_code.unwrap_or(StatusCode::OK);
        if !status.is_client_error()
            && !status.is_server_error()
            && !status.is_redirection()
            && status != StatusCode::NO_CONTENT
            && status != StatusCode::SWITCHING_PROTOCOLS
        {
            res.render("");
        }
    }

}

pub fn post_handler() -> Router {
    let post_router = Router::new()
        .post(create);
    post_router
}
