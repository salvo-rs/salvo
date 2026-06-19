use std::sync::Arc;

use futures_util::StreamExt;
use salvo_core::http::{HeaderValue, StatusCode};
use salvo_core::{Depot, Request, Response, Router, handler};

use crate::error::{ProtocolError, TusError};
use crate::handlers::apply_common_headers;
use crate::stores::Extension;
use crate::utils::{check_tus_version, parse_u64};
use crate::{
    CT_OFFSET_OCTET_STREAM, CancellationContext, H_CONTENT_LENGTH, H_CONTENT_TYPE, H_TUS_RESUMABLE,
    H_TUS_VERSION, H_UPLOAD_EXPIRES, H_UPLOAD_LENGTH, H_UPLOAD_OFFSET, TUS_VERSION, Tus,
};

#[handler]
async fn patch(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get_typed::<Arc<Tus>>().expect("missing tus state");
    let opts = &state.options;
    let store = &state.store;
    apply_common_headers(req, opts, &mut res.headers);

    let id = match opts.extract_file_id_from_request(req) {
        Ok(id) => id,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    // 1. Check TUS version.
    if let Err(e) = check_tus_version(
        req.headers()
            .get(H_TUS_RESUMABLE)
            .and_then(|v| v.to_str().ok()),
    ) {
        if matches!(e, ProtocolError::UnsupportedTusVersion(_)) {
            res.headers
                .insert(H_TUS_VERSION, HeaderValue::from_static(TUS_VERSION));
        }
        res.status_code = Some(TusError::Protocol(e).status());
        return;
    }

    // 2. Check Content Type. The request MUST include a Content-Type header
    let content_type = req
        .headers()
        .get(H_CONTENT_TYPE)
        .and_then(|v| v.to_str().ok());
    if content_type != Some(CT_OFFSET_OCTET_STREAM) {
        res.status_code = Some(TusError::Protocol(ProtocolError::InvalidContentType).status());
        return;
    }

    // 3. Check Upload-Offset. The request MUST include a Upload-Offset header
    let offset = match parse_u64(
        req.headers()
            .get(H_UPLOAD_OFFSET)
            .and_then(|v| v.to_str().ok()),
        H_UPLOAD_OFFSET,
    ) {
        Ok(offset) => offset,
        Err(e) => {
            res.status_code = Some(TusError::Protocol(e).status());
            return;
        }
    };

    if let Some(on_incoming_request) = &opts.on_incoming_request {
        on_incoming_request(req, id.clone()).await;
    }

    let max_file_size = opts.get_configured_max_size(req, Some(id.clone())).await;
    let _lock = match opts
        .acquire_write_lock(req, &id, CancellationContext::new())
        .await
    {
        Ok(lock) => lock,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    let mut already_uploaded_info = match store.get_upload_file_info(&id).await {
        Ok(info) => info,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    let mut expires_at = None;
    if store.has_extension(Extension::Expiration)
        && let Some(expiration) = store.get_expiration()
        && expiration > std::time::Duration::from_secs(0)
        && !already_uploaded_info.creation_date.is_empty()
        && let Ok(created_at) =
            chrono::DateTime::parse_from_rfc3339(&already_uploaded_info.creation_date)
        && let Ok(delta) = chrono::Duration::from_std(expiration)
    {
        let expires = created_at.with_timezone(&chrono::Utc) + delta;
        if chrono::Utc::now() > expires {
            res.status_code = Some(TusError::FileNoLongerExists.status());
            return;
        }
        expires_at = Some(expires);
    }

    // If a Client does attempt to resume an upload which has since
    // been removed by the Server, the Server SHOULD respond with the
    // with the 404 Not Found or 410 Gone status. The latter one SHOULD
    // be used if the Server is keeping track of expired uploads.

    // 404: deleted
    // 410: expiration

    // TODO: Time handle

    let Some(uploaded_info_offset) = already_uploaded_info.offset else {
        res.status_code = Some(TusError::InvalidOffset.status());
        return;
    };

    if uploaded_info_offset != offset {
        tracing::info!(
            "Incorrect offset - {:?} sent but file is {:?}",
            offset,
            uploaded_info_offset
        );
        res.status_code = Some(TusError::InvalidOffset.status());
        return;
    }

    if let Some(raw_length) = req.headers().get(H_UPLOAD_LENGTH) {
        let size = if let Ok(value) = raw_length.to_str() {
            match parse_u64(Some(value), H_UPLOAD_LENGTH) {
                Ok(size) => size,
                Err(e) => {
                    res.status_code = Some(TusError::Protocol(e).status());
                    return;
                }
            }
        } else {
            res.status_code =
                Some(TusError::Protocol(ProtocolError::InvalidInt(H_UPLOAD_LENGTH)).status());
            return;
        };

        if !store.has_extension(Extension::CreationDeferLength) {
            res.status_code = Some(
                TusError::Protocol(ProtocolError::UnsupportedCreationDeferLengthExtension).status(),
            );
            return;
        }
        // Return if upload-length is already set.
        if already_uploaded_info.size.is_some() {
            res.status_code = Some(TusError::Protocol(ProtocolError::InvalidLength).status());
            return;
        }

        if size < uploaded_info_offset {
            res.status_code = Some(TusError::Protocol(ProtocolError::InvalidLength).status());
            return;
        }

        if max_file_size > 0 && size > max_file_size {
            res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
            return;
        }

        if let Err(e) = store.declare_upload_length(&id, size).await {
            res.status_code = Some(e.status());
            return;
        }
        already_uploaded_info.size = Some(size);
    }

    let content_length = match req.headers().get(H_CONTENT_LENGTH) {
        Some(value) => {
            if let Ok(v) = value.to_str() {
                match parse_u64(Some(v), H_CONTENT_LENGTH) {
                    Ok(size) => Some(size),
                    Err(e) => {
                        res.status_code = Some(TusError::Protocol(e).status());
                        return;
                    }
                }
            } else {
                res.status_code =
                    Some(TusError::Protocol(ProtocolError::InvalidInt(H_CONTENT_LENGTH)).status());
                return;
            }
        }
        None => None,
    };

    let max_allowed = match (already_uploaded_info.size, max_file_size) {
        (Some(size), max) if max > 0 => Some(size.min(max)),
        (Some(size), _) => Some(size),
        (None, max) if max > 0 => Some(max),
        _ => None,
    };

    let remaining_u64_capacity = u64::MAX - offset;
    let max_write_size = match max_allowed {
        Some(max_allowed) if offset > max_allowed => {
            res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
            return;
        }
        Some(max_allowed) => Some((max_allowed - offset).min(remaining_u64_capacity)),
        None => Some(remaining_u64_capacity),
    };

    if let Some(incoming) = content_length {
        let Some(end) = offset.checked_add(incoming) else {
            res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
            return;
        };
        if max_allowed.is_some_and(|max_allowed| end > max_allowed) {
            res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
            return;
        }
    }

    // let max_body_size = opts.calculate_max_body_size(req, already_uploaded_info,
    // max_file_size).await; let new_offset = store.write(req.body, already_uploaded_info,
    // max_body_size, context);

    let body = req.take_body();
    let stream = body.map(|frame| frame.map(|frame| frame.into_data().unwrap_or_default()));
    let written = match store
        .write_limited(&id, offset, Box::pin(stream), max_write_size)
        .await
    {
        Ok(written) => written,
        Err(e) => {
            res.status_code = Some(e.status());
            return;
        }
    };

    let Some(new_offset) = offset.checked_add(written) else {
        res.status_code = Some(TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status());
        return;
    };

    if let Some(expires_at) = expires_at {
        let is_finished = match already_uploaded_info.size {
            Some(size) => new_offset == size,
            None => false,
        };

        if !is_finished {
            let expires_value = expires_at.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
            if let Ok(v) = HeaderValue::from_str(&expires_value) {
                res.headers.insert(H_UPLOAD_EXPIRES, v);
            }
        }
    }

    // The Server MUST acknowledge successful PATCH requests with the 204 No Content status.
    // It MUST include the Upload-Offset header containing the new offset.
    // The new offset MUST be the sum of the offset before the PATCH request and the number of bytes
    // received and processed or stored during the current PATCH request.
    res.status_code = Some(StatusCode::NO_CONTENT);
    res.headers
        .insert(H_UPLOAD_OFFSET, HeaderValue::from(new_offset));
}

pub(crate) fn patch_handler() -> Router {
    Router::with_path("{id}").patch(patch)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use salvo_core::http::StatusCode;
    use salvo_core::test::TestClient;
    use salvo_core::{Service, async_trait};

    use super::*;
    use crate::stores::{ByteStream, DataStore, UploadInfo};
    use crate::{Extension, MaxSize, TusError};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ObservedWriteLimit {
        NotCalled,
        Unlimited,
        Limited(u64),
    }

    struct PatchTestStore {
        info: UploadInfo,
        declare_result: Result<(), TusError>,
        written: u64,
        declare_called: Arc<AtomicBool>,
        write_called: Arc<AtomicBool>,
        write_limit: Arc<Mutex<ObservedWriteLimit>>,
    }

    #[async_trait]
    impl DataStore for PatchTestStore {
        fn extensions(&self) -> HashSet<Extension> {
            HashSet::from([Extension::CreationDeferLength])
        }

        async fn create(&self, file: UploadInfo) -> crate::error::TusResult<UploadInfo> {
            Ok(file)
        }

        async fn remove(&self, _id: &str) -> crate::error::TusResult<()> {
            Ok(())
        }

        async fn write(
            &self,
            _id: &str,
            _offset: u64,
            _stream: ByteStream,
        ) -> crate::error::TusResult<u64> {
            self.write_called.store(true, Ordering::SeqCst);
            Ok(self.written)
        }

        async fn write_limited(
            &self,
            id: &str,
            offset: u64,
            stream: ByteStream,
            max_bytes: Option<u64>,
        ) -> crate::error::TusResult<u64> {
            *self.write_limit.lock().expect("write limit lock") = match max_bytes {
                Some(max_bytes) => ObservedWriteLimit::Limited(max_bytes),
                None => ObservedWriteLimit::Unlimited,
            };
            self.write(id, offset, stream).await
        }

        async fn get_upload_file_info(&self, _id: &str) -> crate::error::TusResult<UploadInfo> {
            Ok(self.info.clone())
        }

        async fn declare_upload_length(
            &self,
            _id: &str,
            _length: u64,
        ) -> crate::error::TusResult<()> {
            self.declare_called.store(true, Ordering::SeqCst);
            self.declare_result
                .as_ref()
                .map(|_| ())
                .map_err(|err| TusError::Internal(err.to_string()))
        }
    }

    fn upload_info(offset: u64, size: Option<u64>) -> UploadInfo {
        UploadInfo {
            id: "upload-id".to_owned(),
            size,
            offset: Some(offset),
            metadata: None,
            storage: None,
            creation_date: "2024-01-01T00:00:00Z".to_owned(),
        }
    }

    #[tokio::test]
    async fn patch_returns_store_error_when_declare_upload_length_fails() {
        let declare_called = Arc::new(AtomicBool::new(false));
        let write_called = Arc::new(AtomicBool::new(false));
        let store = PatchTestStore {
            info: upload_info(0, None),
            declare_result: Err(TusError::Internal("declare failed".to_owned())),
            written: 4,
            declare_called: declare_called.clone(),
            write_called: write_called.clone(),
            write_limit: Arc::new(Mutex::new(ObservedWriteLimit::NotCalled)),
        };
        let service = Service::new(
            Tus::new()
                .max_size(MaxSize::Fixed(u64::MAX))
                .store(store)
                .into_router(),
        );

        let response = TestClient::patch("http://localhost/tus-files/upload-id")
            .add_header(H_TUS_RESUMABLE, TUS_VERSION, true)
            .add_header(H_CONTENT_TYPE, CT_OFFSET_OCTET_STREAM, true)
            .add_header(H_UPLOAD_OFFSET, "0", true)
            .add_header(H_UPLOAD_LENGTH, "4", true)
            .add_header(H_CONTENT_LENGTH, "4", true)
            .body("data")
            .send(&service)
            .await;

        assert_eq!(
            response.status_code.unwrap(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert!(declare_called.load(Ordering::SeqCst));
        assert!(!write_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn patch_rejects_content_length_overflow_before_writing() {
        let declare_called = Arc::new(AtomicBool::new(false));
        let write_called = Arc::new(AtomicBool::new(false));
        let store = PatchTestStore {
            info: upload_info(u64::MAX, None),
            declare_result: Ok(()),
            written: 1,
            declare_called,
            write_called: write_called.clone(),
            write_limit: Arc::new(Mutex::new(ObservedWriteLimit::NotCalled)),
        };
        let service = Service::new(
            Tus::new()
                .max_size(MaxSize::Fixed(u64::MAX))
                .store(store)
                .into_router(),
        );

        let response = TestClient::patch("http://localhost/tus-files/upload-id")
            .add_header(H_TUS_RESUMABLE, TUS_VERSION, true)
            .add_header(H_CONTENT_TYPE, CT_OFFSET_OCTET_STREAM, true)
            .add_header(H_UPLOAD_OFFSET, u64::MAX.to_string(), true)
            .add_header(H_CONTENT_LENGTH, "1", true)
            .body("x")
            .send(&service)
            .await;

        assert_eq!(response.status_code.unwrap(), StatusCode::PAYLOAD_TOO_LARGE);
        assert!(!write_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn patch_caps_unbounded_write_size_to_remaining_u64_capacity() {
        let write_limit = Arc::new(Mutex::new(ObservedWriteLimit::NotCalled));
        let write_called = Arc::new(AtomicBool::new(false));
        let store = PatchTestStore {
            info: upload_info(u64::MAX, None),
            declare_result: Ok(()),
            written: 0,
            declare_called: Arc::new(AtomicBool::new(false)),
            write_called: write_called.clone(),
            write_limit: write_limit.clone(),
        };
        let service = Service::new(
            Tus::new()
                .max_size(MaxSize::Fixed(u64::MAX))
                .store(store)
                .into_router(),
        );

        let response = TestClient::patch("http://localhost/tus-files/upload-id")
            .add_header(H_TUS_RESUMABLE, TUS_VERSION, true)
            .add_header(H_CONTENT_TYPE, CT_OFFSET_OCTET_STREAM, true)
            .add_header(H_UPLOAD_OFFSET, u64::MAX.to_string(), true)
            .add_header(H_CONTENT_LENGTH, "0", true)
            .body("")
            .send(&service)
            .await;

        assert_eq!(response.status_code.unwrap(), StatusCode::NO_CONTENT);
        assert!(write_called.load(Ordering::SeqCst));
        assert_eq!(
            *write_limit.lock().expect("write limit lock"),
            ObservedWriteLimit::Limited(0)
        );
    }
}
