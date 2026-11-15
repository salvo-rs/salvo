//! Function attributes remain meaningful on public generated handler items.
#![deny(missing_docs, deprecated)]
use salvo_core::Router;
use salvo_oapi::{OpenApi, endpoint};
use serde_json::json;

#[salvo_core::handler]
#[allow(missing_docs)]
#[inline]
pub fn allowed_handler() -> &'static str {
    "handler"
}
#[endpoint]
#[allow(missing_docs)]
#[cold]
pub fn allowed_endpoint() -> &'static str {
    "endpoint"
}

#[salvo_core::handler]
#[cfg_attr(all(), allow(missing_docs))]
pub fn conditional_lint() -> &'static str {
    "conditional"
}
/// A conditional deprecation attribute has the same public semantics.
#[endpoint]
#[cfg_attr(all(), deprecated(note = "use allowed_endpoint"))]
pub fn conditional_old() -> &'static str {
    "old"
}

/// An old handler can be declared without an internal deprecation error.
#[salvo_core::handler]
#[deprecated(note = "use allowed_handler")]
pub fn old_handler() -> &'static str {
    "old"
}
/// An old endpoint can be declared without an internal deprecation error.
#[endpoint]
#[deprecated(note = "use allowed_endpoint")]
pub fn old_endpoint() -> &'static str {
    "old"
}

#[test]
#[allow(deprecated)]
fn public_lints_and_document_deprecation_are_preserved() {
    let router = Router::new()
        .push(Router::with_path("handler").get(allowed_handler))
        .push(Router::with_path("endpoint").get(allowed_endpoint))
        .push(Router::with_path("old-handler").get(old_handler))
        .push(Router::with_path("old-endpoint").get(old_endpoint));
    let doc = json!(OpenApi::new("attributes", "1").merge_router(&router));
    assert_eq!(doc["paths"]["/old-endpoint"]["get"]["deprecated"], true);
}
