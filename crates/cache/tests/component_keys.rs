use salvo_cache::{CacheIssuer, RequestIssuer};
use salvo_core::Depot;
use salvo_core::http::Method;
use salvo_core::test::TestClient;

#[tokio::test]
async fn extension_methods_cannot_collide_with_path_components() {
    let issuer = RequestIssuer::new();
    let mut extension = TestClient::get("http://localhost/item").build();
    *extension.method_mut() = Method::from_bytes(b"FOO|GET").unwrap();
    let mut get = TestClient::get("http://localhost/item|FOO").build();
    assert_ne!(
        issuer.issue(&mut extension, &Depot::new()).await,
        issuer.issue(&mut get, &Depot::new()).await
    );
}

#[tokio::test]
async fn component_boundaries_remain_distinct_with_issuer_switches() {
    let issuer = RequestIssuer::new().use_scheme(false).use_authority(false);
    let mut one = TestClient::get("http://localhost/a?b").build();
    let mut two = TestClient::get("http://localhost/ab").build();
    assert_ne!(
        issuer.issue(&mut one, &Depot::new()).await,
        issuer.issue(&mut two, &Depot::new()).await
    );
}
