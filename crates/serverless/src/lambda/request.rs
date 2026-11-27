//! Conversion from Lambda HTTP events to Salvo requests.

use bytes::Bytes;
use lambda_http::aws_lambda_events::query_map::QueryMap;
use lambda_http::request::RequestContext;
use lambda_http::{Context, RequestExt as LambdaEventExt};
use salvo_core::conn::SocketAddr;
use salvo_core::http::uri::Scheme;
use salvo_core::http::{ReqBody, Request as SalvoRequest};

#[cfg(test)]
const API_GW_FIXTURE: &str = r#"{
    "version": "2.0",
    "rawPath": "/hello",
    "rawQueryString": "foo=bar",
    "pathParameters": { "id": "42" },
    "stageVariables": { "env": "prod" },
    "headers": {
        "host": "example.com",
        "content-type": "text/plain",
        "x-forwarded-for": "203.0.113.7, 10.0.0.1",
        "x-forwarded-proto": "https"
    },
    "requestContext": {
        "accountId": "123456789012",
        "apiId": "test",
        "domainName": "example.com",
        "domainPrefix": "test",
        "http": {
            "method": "GET",
            "path": "/hello",
            "protocol": "HTTP/1.1",
            "sourceIp": "203.0.113.7",
            "userAgent": "test"
        },
        "requestId": "request-id",
        "routeKey": "$default",
        "stage": "$default",
        "time": "18/Nov/2026:00:00:00 +0000",
        "timeEpoch": 1
    },
    "isBase64Encoded": false,
    "body": "hello lambda"
}"#;

/// Converts a Lambda HTTP request into a Salvo request.
///
/// The scheme is taken from `x-forwarded-proto` first, then from the URI, and
/// falls back to HTTP. The remote address is the first `x-forwarded-for`
/// entry, or [`SocketAddr::Unknown`] when it cannot be parsed. Request
/// extensions, including the Lambda context exposed by
/// `lambda_http::RequestExt`, are preserved.
pub(crate) fn into_salvo_request(req: lambda_http::Request) -> SalvoRequest {
    let (parts, body) = req.into_parts();

    let req_body = match body {
        lambda_http::Body::Text(text) => ReqBody::Once(Bytes::from(text)),
        lambda_http::Body::Binary(bytes) => ReqBody::Once(Bytes::from(bytes)),
        _ => ReqBody::None,
    };

    let scheme = parts
        .headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .or_else(|| parts.uri.scheme_str())
        .and_then(|value| value.parse::<Scheme>().ok())
        .unwrap_or(Scheme::HTTP);

    let remote_addr = parts
        .headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse::<std::net::IpAddr>().ok())
        .map_or(SocketAddr::Unknown, |ip| {
            SocketAddr::from(std::net::SocketAddr::new(ip, 0))
        });

    let hyper_req = hyper::Request::from_parts(parts, req_body);
    let mut request = SalvoRequest::from_hyper(hyper_req, scheme);
    *request.remote_addr_mut() = remote_addr;
    request
}

/// Extension trait that exposes Lambda event data attached to a Salvo request.
///
/// The Lambda adapter preserves the request extensions populated by
/// `lambda_http`, so handlers can read the Lambda execution context, the API
/// Gateway request context, pre-parsed query/path parameters and stage
/// variables without depending on `lambda_http` directly.
pub trait LambdaRequestExt {
    /// Returns the Lambda execution context, when attached by the runtime.
    fn lambda_context(&self) -> Option<&Context>;

    /// Returns the API Gateway / ALB request context, when available.
    fn request_context(&self) -> Option<&RequestContext>;

    /// Returns the pre-parsed query string parameters.
    fn query_string_parameters(&self) -> Option<&QueryMap>;

    /// Returns the pre-extracted URL path parameters.
    fn path_parameters(&self) -> Option<&QueryMap>;

    /// Returns the API Gateway stage variables.
    fn stage_variables(&self) -> Option<&QueryMap>;

    /// Returns the raw HTTP path without any stage information.
    fn raw_http_path(&self) -> &str;
}

impl LambdaRequestExt for SalvoRequest {
    fn lambda_context(&self) -> Option<&Context> {
        self.extensions().lambda_context_ref()
    }

    fn request_context(&self) -> Option<&RequestContext> {
        self.extensions().request_context_ref()
    }

    fn query_string_parameters(&self) -> Option<&QueryMap> {
        self.extensions().query_string_parameters_ref()
    }

    fn path_parameters(&self) -> Option<&QueryMap> {
        self.extensions().path_parameters_ref()
    }

    fn stage_variables(&self) -> Option<&QueryMap> {
        self.extensions().stage_variables_ref()
    }

    fn raw_http_path(&self) -> &str {
        self.extensions().raw_http_path()
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::http::Method;
    use salvo_core::http::uri::Scheme;

    use super::*;

    #[test]
    fn converts_method_path_headers_and_scheme() {
        let req = lambda_http::request::from_str(API_GW_FIXTURE).unwrap();
        let request = into_salvo_request(req);
        assert_eq!(request.method(), Method::GET);
        assert_eq!(request.uri().path(), "/hello");
        assert_eq!(request.scheme(), &Scheme::HTTPS);
        assert_eq!(request.headers().get("content-type").unwrap(), "text/plain");
        assert_eq!(
            request.remote_addr().ip(),
            Some("203.0.113.7".parse::<std::net::IpAddr>().unwrap())
        );
    }

    #[test]
    fn converts_text_body() {
        let req = lambda_http::request::from_str(API_GW_FIXTURE).unwrap();
        let request = into_salvo_request(req);
        match request.body() {
            ReqBody::Once(bytes) => assert_eq!(bytes.as_ref(), b"hello lambda"),
            other => panic!("unexpected body: {other:?}"),
        }
    }

    #[test]
    fn exposes_lambda_event_data() {
        let req = lambda_http::request::from_str(API_GW_FIXTURE).unwrap();
        let request = into_salvo_request(req);
        assert_eq!(request.raw_http_path(), "/hello");
        assert!(request.request_context().is_some());
        assert!(request.lambda_context().is_none());
    }

    #[test]
    fn exposes_query_string_parameters() {
        let req = lambda_http::request::from_str(API_GW_FIXTURE).unwrap();
        let request = into_salvo_request(req);
        assert!(request.query_string_parameters().is_some());
        assert!(request.path_parameters().is_some());
        assert!(request.stage_variables().is_some());
    }
}
