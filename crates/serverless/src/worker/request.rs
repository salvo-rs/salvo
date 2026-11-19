//! Conversion from Cloudflare Workers HTTP requests to Salvo requests.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use hyper::body::{Body, Frame, SizeHint};
use salvo_core::conn::SocketAddr;
use salvo_core::http::uri::Scheme;
use salvo_core::http::{ReqBody, Request as SalvoRequest};
use salvo_core::BoxedError;

/// Wraps a Workers request body stream so it can live in a boxed [`ReqBody`].
///
/// 将 Workers 请求体流包装为可存入 `ReqBody::Boxed` 的类型，并把 Workers 专用
/// 错误映射为 Salvo 的类型擦除错误，从而避免在进入处理器前一次性缓存整个请求体。
/// The Workers-specific error is mapped to Salvo's boxed error, so the incoming
/// body can be streamed into the handler instead of buffered all at once.
struct StreamedWorkerBody(worker::Body);

impl Body for StreamedWorkerBody {
    type Data = Bytes;
    type Error = BoxedError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, BoxedError>>> {
        // `worker::Body`（及其内部的 `IntoStream`）为 `Unpin`，`StreamedWorkerBody`
        // 因此也是 `Unpin`，可安全地重新借用内部字段进行轮询，无需 `unsafe` 投影。
        // `worker::Body` (and the wrapped `IntoStream`) is `Unpin`, which makes this
        // wrapper `Unpin` too, so the inner field can be safely re-borrowed without an
        // `unsafe` projection.
        let this = self.get_mut();
        Pin::new(&mut this.0).poll_frame(cx).map(|frame| {
            frame.map(|result| result.map_err(|err| Box::new(err) as BoxedError))
        })
    }

    fn size_hint(&self) -> SizeHint {
        self.0.size_hint()
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }
}

/// Converts a Workers HTTP request into a Salvo request.
///
/// The scheme is resolved from `CF-Visitor` first, then `x-forwarded-proto`,
/// then the URI, and finally defaults to HTTPS. The remote address is taken
/// from `CF-Connecting-IP`, `True-Client-IP` or the first `X-Forwarded-For`
/// entry; when none of them parse, [`SocketAddr::Unknown`] is used. The
/// [`worker::Env`] and an `Arc<worker::Context>` are stored in the request
/// extensions so handlers can reach them through
/// [`WorkerRequestExt`](crate::worker::WorkerRequestExt).
pub(crate) async fn into_salvo_request(
    req: worker::HttpRequest,
    env: &worker::Env,
    ctx: &Arc<worker::Context>,
) -> worker::Result<SalvoRequest> {
    let (parts, body) = req.into_parts();

    // 将 Workers 请求体直接包装为流式 `ReqBody`，避免一次性读入内存，使大文件上传
    // 可以边读边转发到对象存储。The Workers body is wrapped as a streaming `ReqBody`
    // instead of being collected upfront, so large uploads can be forwarded chunk by chunk.
    let req_body = ReqBody::boxed(StreamedWorkerBody(body));

    let scheme = resolve_scheme(&parts.headers, parts.uri.scheme_str());
    let remote_addr = resolve_remote_addr(&parts.headers);

    let hyper_req = hyper::Request::from_parts(parts, req_body);
    let mut request = SalvoRequest::from_hyper(hyper_req, scheme);
    *request.remote_addr_mut() = remote_addr;

    request.extensions_mut().insert(env.clone());
    request.extensions_mut().insert(ctx.clone());

    Ok(request)
}

fn resolve_scheme(headers: &http::HeaderMap, uri_scheme: Option<&str>) -> Scheme {
    headers
        .get("cf-visitor")
        .and_then(|value| value.to_str().ok())
        .and_then(cf_visitor_scheme)
        .or_else(|| {
            headers
                .get("x-forwarded-proto")
                .and_then(|value| value.to_str().ok())
                .and_then(parse_scheme)
        })
        .or_else(|| uri_scheme.and_then(parse_scheme))
        .unwrap_or(Scheme::HTTPS)
}

fn cf_visitor_scheme(value: &str) -> Option<Scheme> {
    let compact: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains("\"scheme\":\"https\"") {
        Some(Scheme::HTTPS)
    } else if compact.contains("\"scheme\":\"http\"") {
        Some(Scheme::HTTP)
    } else {
        None
    }
}

fn parse_scheme(value: &str) -> Option<Scheme> {
    value.split(',').next()?.trim().parse::<Scheme>().ok()
}

fn resolve_remote_addr(headers: &http::HeaderMap) -> SocketAddr {
    headers
        .get("cf-connecting-ip")
        .and_then(|value| value.to_str().ok())
        .or_else(|| {
            headers
                .get("true-client-ip")
                .and_then(|value| value.to_str().ok())
        })
        .or_else(|| {
            headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
        })
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse::<std::net::IpAddr>().ok())
        .map_or(SocketAddr::Unknown, |ip| {
            SocketAddr::from(std::net::SocketAddr::new(ip, 0))
        })
}

#[cfg(test)]
mod tests {
    use salvo_core::http::HeaderMap;

    use super::*;

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn resolves_cf_visitor_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-visitor", "{\"scheme\":\"http\"}".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        assert_eq!(resolve_scheme(&headers, Some("https")), Scheme::HTTP);
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn resolves_forwarded_proto_and_uri() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", "http".parse().unwrap());
        assert_eq!(resolve_scheme(&headers, Some("https")), Scheme::HTTP);

        headers.clear();
        assert_eq!(resolve_scheme(&headers, Some("https")), Scheme::HTTPS);
        assert_eq!(resolve_scheme(&headers, None), Scheme::HTTPS);
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn resolves_remote_addr_priority() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", "203.0.113.9".parse().unwrap());
        headers.insert("true-client-ip", "203.0.113.8".parse().unwrap());
        headers.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
        match resolve_remote_addr(&headers) {
            SocketAddr::Unknown => panic!("expected a known address"),
            addr => assert_eq!(
                addr.ip(),
                Some("203.0.113.9".parse::<std::net::IpAddr>().unwrap())
            ),
        }
    }
}