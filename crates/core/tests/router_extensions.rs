//! Construction metadata follows router ownership without entering request routing.
use std::sync::Arc;

use salvo_core::Router;

#[test]
fn router_extensions_release_owned_metadata_on_drop() {
    let metadata = Arc::new(String::from("router-only metadata"));
    let weak = Arc::downgrade(&metadata);
    let mut router = Router::new();
    router.extensions_mut().insert(metadata);
    assert_eq!(
        &**router.extensions().get::<Arc<String>>().unwrap(),
        "router-only metadata"
    );
    assert!(router.hoops.is_empty());
    assert!(router.filters.is_empty());
    drop(router);
    assert!(weak.upgrade().is_none());
}
