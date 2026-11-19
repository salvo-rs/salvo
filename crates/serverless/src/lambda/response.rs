//! Conversion from Salvo responses to Lambda HTTP responses.

use http_body_util::BodyExt;
use salvo_core::http::{ResBody, Response as SalvoResponse, StatusCode};

/// Converts a Salvo response into a Lambda HTTP response.
///
/// All body representations are buffered because the Lambda response must be
/// a single [`lambda_http::Body`] value.
pub(crate) async fn from_salvo_response(
    mut response: SalvoResponse,
) -> lambda_http::Response<lambda_http::Body> {
    let body = collect_body(std::mem::take(&mut response.body)).await;
    let mut result = lambda_http::http::Response::new(lambda_http::Body::from(body));
    *result.status_mut() = response.status_code.unwrap_or(StatusCode::OK);
    *result.version_mut() = response.version;
    *result.headers_mut() = std::mem::take(response.headers_mut());
    result
}

/// Converts a Salvo response into a streaming Lambda HTTP response.
///
/// The [`ResBody`] is preserved as the response body so `lambda_http` forwards
/// chunks to the Lambda runtime incrementally instead of buffering the whole
/// payload. Pass the returned value to `run_with_streaming_response`.
pub(crate) fn from_salvo_response_streaming(
    mut response: SalvoResponse,
) -> lambda_http::Response<ResBody> {
    let mut result = lambda_http::http::Response::new(std::mem::take(&mut response.body));
    *result.status_mut() = response.status_code.unwrap_or(StatusCode::OK);
    *result.version_mut() = response.version;
    *result.headers_mut() = std::mem::take(response.headers_mut());
    result
}

async fn collect_body(body: ResBody) -> Vec<u8> {
    match body {
        ResBody::None | ResBody::Error(_) => Vec::new(),
        ResBody::Once(bytes) => bytes.to_vec(),
        ResBody::Chunks(chunks) => chunks.into_iter().fold(Vec::new(), |mut bytes, chunk| {
            bytes.extend_from_slice(&chunk);
            bytes
        }),
        body => match body.collect().await {
            Ok(collected) => collected.to_bytes().to_vec(),
            Err(_) => Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use bytes::Bytes;

    use super::*;

    #[tokio::test]
    async fn converts_status_version_headers_and_body() {
        let mut response = SalvoResponse::new();
        response.status_code = Some(StatusCode::CREATED);
        response
            .headers_mut()
            .insert("x-test", "yes".parse().unwrap());
        response.body = ResBody::Once(Bytes::from_static(b"created"));

        let result = from_salvo_response(response).await;
        assert_eq!(result.status(), StatusCode::CREATED);
        assert_eq!(result.headers().get("x-test").unwrap(), "yes");
        assert_eq!(result.body().as_ref(), b"created");
    }

    #[tokio::test]
    async fn collects_chunked_body() {
        let mut response = SalvoResponse::new();
        response.body = ResBody::Chunks(VecDeque::from(vec![
            Bytes::from_static(b"ab"),
            Bytes::from_static(b"cd"),
        ]));

        let result = from_salvo_response(response).await;
        assert_eq!(result.status(), StatusCode::OK);
        assert_eq!(result.body().as_ref(), b"abcd");
    }

    #[test]
    fn streaming_response_keeps_body_unbuffered() {
        let mut response = SalvoResponse::new();
        response.status_code = Some(StatusCode::ACCEPTED);
        response.body = ResBody::Once(Bytes::from_static(b"streamed"));

        let result = from_salvo_response_streaming(response);
        assert_eq!(result.status(), StatusCode::ACCEPTED);
        match result.into_body() {
            ResBody::Once(bytes) => assert_eq!(bytes.as_ref(), b"streamed"),
            other => panic!("expected a non-buffered ResBody::Once, got {other:?}"),
        }
    }
}
