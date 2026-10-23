//! Middleware for handling ETag and Last-Modified headers.
//!
//! This crate provides three handlers: [`ETag`], [`Modified`], and [`CachingHeaders`].
//! Unless you are sure that you _don't_ want either ETag or Last-Modified
//! behavior, use the combined [`CachingHeaders`] handler for better cache control.
//!
//! Conditional responses apply only to successful `200 OK` GET/HEAD responses.
//! Preconditions for writes must be checked by the application before performing
//! side effects; these handlers do not validate write preconditions.

use etag::EntityTag;
use salvo_core::http::header::{ETAG, IF_NONE_MATCH};
use salvo_core::http::headers::{self, Header, HeaderMapExt};
use salvo_core::http::{Method, ResBody, StatusCode};
use salvo_core::{Depot, FlowCtrl, Handler, Request, Response, async_trait};

fn can_revalidate(req: &Request, res: &Response) -> bool {
    matches!(*req.method(), Method::GET | Method::HEAD)
        && res.status_code.unwrap_or(if res.body.is_none() {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::OK
        }) == StatusCode::OK
        && !res.body.is_error()
}

/// Etag and If-None-Match header handler
///
/// Salvo handler that provides an outbound [`etag
/// header`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/ETag)
/// after other handlers have been run, and if the request includes an
/// [`if-none-match`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/If-None-Match)
/// header, compares these values and sends a
/// [`304 not modified`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/304) status,
/// omitting the response body.
///
/// ## Streamed bodies
///
/// **Note**: This handler does not currently provide an etag trailer for
/// streamed bodies, but may do so in the future.
///
/// ## Strong vs weak comparison
///
/// Etags can be compared using a strong method or a weak
/// method. By default, this handler allows weak comparison. To change
/// this setting, construct your handler with `Etag::new().strong()`.
/// See [`etag::EntityTag`](https://docs.rs/etag/3.0.0/etag/struct.EntityTag.html#comparison)
/// for further documentation.
#[derive(Default, Clone, Copy, Debug)]
pub struct ETag {
    strong: bool,
}

impl ETag {
    /// Constructs a new `ETag` handler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures this handler to use strong content-based etag comparison only. See
    /// [`etag::EntityTag`](https://docs.rs/etag/3.0.0/etag/struct.EntityTag.html#comparison)
    /// for further documentation on the differences between strong
    /// and weak etag comparison.
    #[must_use]
    pub fn strong(mut self) -> Self {
        self.strong = true;
        self
    }
}

#[async_trait]
impl Handler for ETag {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        ctrl.call_next(req, depot, res).await;
        if ctrl.is_ceased() || !can_revalidate(req, res) {
            return;
        }

        // Prefer an ETag already set on the *response* by an upstream handler.
        // (Reading it from the request is wrong: `ETag` is a response header, and
        // a client could forge it to control caching and force a 304.)
        let res_etag = res
            .headers()
            .get(ETAG)
            .and_then(|etag| etag.to_str().ok())
            .and_then(|etag| etag.parse::<EntityTag>().ok());

        let etag = res_etag.or_else(|| {
            let etag = match &res.body {
                ResBody::Once(bytes) => Some(EntityTag::from_data(bytes)),
                ResBody::Chunks(bytes) => {
                    let tags = bytes
                        .iter()
                        .map(|item| EntityTag::from_data(item).tag().to_owned())
                        .collect::<Vec<_>>()
                        .concat();
                    Some(EntityTag::from_data(tags.as_bytes()))
                }
                ResBody::Stream(_) => {
                    tracing::debug!("etag not supported for streaming body");
                    None
                }
                ResBody::None => {
                    tracing::debug!("etag not supported for empty body");
                    None
                }
                _ => None,
            };

            if let Some(etag) = &etag {
                match etag.to_string().parse::<headers::ETag>() {
                    Ok(etag) => res.headers_mut().typed_insert(etag),
                    Err(e) => {
                        tracing::error!(error = ?e, "failed to parse etag");
                    }
                }
            }
            etag
        });

        let etag = etag.and_then(|etag| etag.to_string().parse::<headers::ETag>().ok());
        // If-Match shares the same field syntax and provides the strong comparison
        // used by the opt-in `strong()` mode. Decode the If-None-Match values,
        // including repeated field lines, rather than reading an If-Match header.
        let tags = headers::IfMatch::decode(&mut req.headers().get_all(IF_NONE_MATCH).iter()).ok();
        let matches = tags.as_ref().is_some_and(|tags| tags.is_any())
            || etag.as_ref().is_some_and(|etag| {
                if self.strong {
                    tags.as_ref()
                        .is_some_and(|tags| tags.precondition_passes(etag))
                } else {
                    req.headers()
                        .typed_get::<headers::IfNoneMatch>()
                        .is_some_and(|tags| !tags.precondition_passes(etag))
                }
            });
        if matches {
            res.body(ResBody::None);
            res.status_code(StatusCode::NOT_MODIFIED);
        }
    }
}

/// # A handler for the `Last-Modified` and `If-Modified-Since` header interaction.
///
/// This handler does not set a `Last-Modified` header on its own, but
/// relies on other handlers doing so.
#[derive(Clone, Debug, Copy, Default)]
pub struct Modified {
    _private: (),
}

impl Modified {
    /// Constructs a new Modified handler
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
    }
}

#[async_trait]
impl Handler for Modified {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        ctrl.call_next(req, depot, res).await;
        if ctrl.is_ceased()
            || !can_revalidate(req, res)
            || req.headers().contains_key(IF_NONE_MATCH)
        {
            return;
        }

        if let (Some(if_modified_since), Some(last_modified)) = (
            req.headers().typed_get::<headers::IfModifiedSince>(),
            res.headers().typed_get::<headers::LastModified>(),
        ) && !if_modified_since.is_modified(last_modified.into())
        {
            res.body(ResBody::None);
            res.status_code(StatusCode::NOT_MODIFIED);
        }
    }
}

/// A combined handler that provides both [`ETag`] and [`Modified`] behavior.
///
/// This handler helps improve performance by preventing unnecessary data transfers
/// when a client already has the latest version of a resource, as determined by
/// either ETag or Last-Modified comparisons.
#[derive(Clone, Debug, Copy, Default)]
pub struct CachingHeaders(Modified, ETag);

impl CachingHeaders {
    /// Constructs a new combination modified and etag handler
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Handler for CachingHeaders {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        self.1.handle(req, depot, res, ctrl).await;
        if res.status_code != Some(StatusCode::NOT_MODIFIED) {
            self.0.handle(req, depot, res, ctrl).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::http::header::*;
    use salvo_core::prelude::*;
    use salvo_core::test::TestClient;

    use super::*;

    #[handler]
    async fn hello() -> &'static str {
        "Hello World"
    }

    const MODIFIED_AT: &str = "Wed, 21 Oct 2015 07:28:00 GMT";

    #[handler]
    async fn versioned(req: &mut Request, res: &mut Response) {
        let code = req.query::<u16>("status").unwrap_or(200);
        res.status_code(StatusCode::from_u16(code).unwrap());
        res.add_header(ETAG, "\"new,tag\"", true).unwrap();
        res.add_header(LAST_MODIFIED, MODIFIED_AT, true).unwrap();
        res.render("current representation");
    }

    #[tokio::test]
    async fn etag_conditions_take_precedence_over_dates() {
        let service = Service::new(Router::with_hoop(CachingHeaders::new()).get(versioned));
        for (condition, expected) in [
            ("\"old\"", StatusCode::OK),
            ("invalid", StatusCode::OK),
            ("\"new,tag\"", StatusCode::NOT_MODIFIED),
        ] {
            let response = TestClient::get("http://localhost/")
                .add_header(IF_NONE_MATCH, condition, true)
                .add_header(IF_MODIFIED_SINCE, MODIFIED_AT, true)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(expected));
            assert_eq!(
                response.body.is_none(),
                expected == StatusCode::NOT_MODIFIED
            );
        }

        let response = TestClient::get("http://localhost/")
            .add_header(IF_MODIFIED_SINCE, MODIFIED_AT, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
        assert_eq!(response.headers()[ETAG], "\"new,tag\"");
    }

    #[tokio::test]
    async fn etag_conditions_support_lists_repeated_fields_and_wildcards() {
        for strong in [false, true] {
            let hoop = if strong {
                ETag::new().strong()
            } else {
                ETag::new()
            };
            let service = Service::new(Router::with_hoop(hoop).goal(versioned));
            for condition in ["\"old\", \"new,tag\"", "*", "W/\"new,tag\""] {
                let response = TestClient::head("http://localhost/")
                    .add_header(IF_NONE_MATCH, condition, true)
                    .send(&service)
                    .await;
                let expected = if strong && condition.starts_with("W/") {
                    StatusCode::OK
                } else {
                    StatusCode::NOT_MODIFIED
                };
                assert_eq!(
                    response.status_code,
                    Some(expected),
                    "{condition}, strong={strong}"
                );
            }
            let response = TestClient::get("http://localhost/")
                .add_header(IF_NONE_MATCH, "\"old\"", true)
                .add_header(IF_NONE_MATCH, "\"new,tag\"", false)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
        }
    }

    #[tokio::test]
    async fn wildcard_matches_a_stream_without_a_generated_etag() {
        #[handler]
        async fn streamed(res: &mut Response) {
            res.status_code(StatusCode::OK);
            res.stream(tokio_stream::iter([Ok::<_, std::io::Error>("hello")]));
        }
        let response = TestClient::get("http://localhost/")
            .add_header(IF_NONE_MATCH, "*", true)
            .send(Router::with_hoop(CachingHeaders::new()).get(streamed))
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
        assert!(response.body.is_none());
    }

    #[tokio::test]
    async fn conditional_headers_preserve_non_ok_responses_and_write_methods() {
        let service = Service::new(Router::with_hoop(CachingHeaders::new()).goal(versioned));
        for status in [204, 206, 301, 401, 404, 500] {
            let response = TestClient::get(format!("http://localhost/?status={status}"))
                .add_header(IF_NONE_MATCH, "\"new,tag\"", true)
                .add_header(IF_MODIFIED_SINCE, MODIFIED_AT, true)
                .send(&service)
                .await;
            assert_eq!(
                response.status_code,
                Some(StatusCode::from_u16(status).unwrap())
            );
        }
        for request in [
            TestClient::post("http://localhost/"),
            TestClient::put("http://localhost/"),
        ] {
            let response = request
                .add_header(IF_NONE_MATCH, "\"new,tag\"", true)
                .add_header(IF_MODIFIED_SINCE, MODIFIED_AT, true)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::OK));
            assert!(!response.body.is_none());
        }
    }

    #[tokio::test]
    async fn test_caching_headers() {
        let router = Router::with_hoop(CachingHeaders::new()).get(hello);
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:8698/")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));

        let etag = response.headers().get(ETAG).unwrap();
        let response = TestClient::get("http://127.0.0.1:8698/")
            .add_header(IF_NONE_MATCH, etag, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
        assert!(response.body.is_none());
    }

    #[tokio::test]
    async fn test_caching_headers_ignores_forged_request_etag() {
        // A client-supplied `ETag` request header must not influence the ETag the
        // server computes from the response body, nor force a 304.
        let router = Router::with_hoop(CachingHeaders::new()).get(hello);
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:8699/")
            .add_header(ETAG, "\"forged\"", true)
            .add_header(IF_NONE_MATCH, "\"forged\"", true)
            .send(&service)
            .await;

        // The real body-derived ETag does not match the forged value, so the
        // response is served normally instead of a spurious 304.
        assert_eq!(response.status_code, Some(StatusCode::OK));
        let etag = response.headers().get(ETAG).unwrap();
        assert_ne!(etag, "\"forged\"");
    }
}
