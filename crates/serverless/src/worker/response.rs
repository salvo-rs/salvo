//! Conversion from Salvo responses to Cloudflare Workers HTTP responses.

use http_body_util::BodyExt;
use salvo_core::http::{Response as SalvoResponse, StatusCode};

use super::WorkerWebSocketHandle;

/// Converts a Salvo response into a Workers HTTP response.
///
/// When the response carries a [`WorkerWebSocketHandle`] extension the
/// conversion hands the websocket to `worker::Response::from_websocket` and
/// returns immediately. Otherwise the body is handed to the Workers runtime
/// as a stream of byte chunks instead of being buffered, so large or
/// indefinite bodies are forwarded incrementally. A [`worker::Body::empty`]
/// is used when the body is absent.
pub(crate) fn into_worker_response(
    mut response: SalvoResponse,
) -> worker::Result<worker::HttpResponse> {
    if let Some(handle) = response.extensions.remove::<WorkerWebSocketHandle>() {
        let websocket_response = worker::Response::from_websocket(handle.0)?;
        return worker::response_from_wasm(websocket_response.into());
    }

    let status = response.status_code.unwrap_or(StatusCode::OK);
    let version = response.version;
    let headers = std::mem::take(response.headers_mut());

    let body = if response.body.is_none() {
        worker::Body::empty()
    } else {
        worker::Body::from_stream(response.body.into_data_stream())?
    };

    let mut result = worker::HttpResponse::new(body);
    *result.status_mut() = status;
    *result.version_mut() = version;
    *result.headers_mut() = headers;

    Ok(result)
}
