//! Middleware for handling ETag and Last-Modified headers.
//!
//! This crate provides three handlers: [`ETag`], [`Modified`], and [`CachingHeaders`].
//! Unless you are sure that you _don't_ want either ETag or Last-Modified
//! behavior, use the combined [`CachingHeaders`] handler for better cache control.

use etag::EntityTag;
use salvo_core::http::header::{ETAG, IF_NONE_MATCH};
use salvo_core::http::headers::{self, HeaderMapExt};
use salvo_core::http::{ResBody, StatusCode};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Request, Response};

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
    /// constructs a new Etag handler
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures this handler to use strong content-based etag comparison only. See
    /// [`etag::EntityTag`](https://docs.rs/etag/3.0.0/etag/struct.EntityTag.html#comparison)
    /// for further documentation on the differences between strong
    /// and weak etag comparison.
    pub fn strong(mut self) -> Self {
        self.strong = true;
        self
    }
}

#[async_trait]
impl Handler for ETag {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        ctrl.call_next(req, depot, res).await;
        if ctrl.is_ceased() {
            return;
        }

        let if_none_match = req
            .headers()
            .get(IF_NONE_MATCH)
            .and_then(|etag| etag.to_str().ok())
            .and_then(|etag| etag.parse::<EntityTag>().ok());

        let etag = req
            .headers()
            .get(ETAG)
            .and_then(|etag| etag.to_str().ok())
            .and_then(|etag| etag.parse().ok())
            .or_else(|| {
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

        if let (Some(etag), Some(if_none_match)) = (etag, if_none_match) {
            let eq = if self.strong {
                etag.strong_eq(&if_none_match)
            } else {
                etag.weak_eq(&if_none_match)
            };

            if eq {
                res.body(ResBody::None);
                res.status_code(StatusCode::NOT_MODIFIED);
            }
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
    pub fn new() -> Self {
        Self { _private: () }
    }
}

#[async_trait]
impl Handler for Modified {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        ctrl.call_next(req, depot, res).await;
        if ctrl.is_ceased() {
            return;
        }

        if let (Some(if_modified_since), Some(last_modified)) = (
            req.headers().typed_get::<headers::IfModifiedSince>(),
            res.headers().typed_get::<headers::LastModified>(),
        ) {
            if !if_modified_since.is_modified(last_modified.into()) {
                res.body(ResBody::None);
                res.status_code(StatusCode::NOT_MODIFIED);
            }
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
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Handler for CachingHeaders {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        self.0.handle(req, depot, res, ctrl).await;
        if res.status_code != Some(StatusCode::NOT_MODIFIED) {
            self.1.handle(req, depot, res, ctrl).await;
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

    #[tokio::test]
    async fn test_affix() {
        let router = Router::with_hoop(CachingHeaders::new()).get(hello);
        let service = Service::new(router);

        let respone = TestClient::get("http://127.0.0.1:5800/").send(&service).await;
        assert_eq!(respone.status_code, Some(StatusCode::OK));

        let etag = respone.headers().get(ETAG).unwrap();
        let respone = TestClient::get("http://127.0.0.1:5800/")
            .add_header(IF_NONE_MATCH, etag, true)
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::NOT_MODIFIED));
        assert!(respone.body.is_none());
    }
}
