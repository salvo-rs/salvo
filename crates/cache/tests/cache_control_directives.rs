//! Cache directives remain authoritative alongside opaque or quoted extensions.
#![cfg(feature = "moka-store")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salvo_cache::{Cache, MokaStore, RequestIssuer};
use salvo_core::http::HeaderValue;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[derive(Debug)]
struct Origin {
    calls: Arc<AtomicUsize>,
    policy: Vec<HeaderValue>,
}

#[async_trait]
impl Handler for Origin {
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        for value in &self.policy {
            res.headers_mut().append("cache-control", value.clone());
        }
        res.render(req.headers()["x-user"].to_str().unwrap().to_owned());
    }
}

async fn verify_policy(policy: &[&[u8]], private: bool, should_cache: bool) {
    let calls = Arc::new(AtomicUsize::new(0));
    let service = Service::new(
        Router::new()
            .hoop(Cache::new(MokaStore::new(100), RequestIssuer::new()).cache_private(private))
            .get(Origin {
                calls: calls.clone(),
                policy: policy
                    .iter()
                    .map(|raw| HeaderValue::from_bytes(raw).unwrap())
                    .collect(),
            }),
    );
    for user in ["alice", "bob"] {
        let mut response = TestClient::get("http://localhost/")
            .add_header("x-user", user, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(
            response.take_string().await.unwrap(),
            if should_cache { "alice" } else { user }
        );
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        if should_cache { 1 } else { 2 }
    );
}

#[tokio::test]
async fn shared_cache_respects_opaque_and_qualified_private_directives() {
    for policy in [
        b"extension=\"\x80\", private".as_slice(),
        b"extension=\"\xff\", \tPrIvAtE=\"x-user, x-other\"\t ".as_slice(),
        b"extension=\"\x80\\\", private\", private".as_slice(),
    ] {
        verify_policy(&[policy], false, false).await;
    }
    verify_policy(&[b"extension=\"\x80\"", b"private"], false, false).await;
    // Opting in to private caching still permits a real private directive.
    verify_policy(&[b"extension=\"\x80\", private"], true, true).await;
}

#[tokio::test]
async fn opaque_no_store_and_no_cache_apply_in_both_cache_modes() {
    for private in [false, true] {
        for policy in [
            b"extension=\"\x80\", no-store".as_slice(),
            b"extension=\"\xff\", \tNO-CACHE=\"x-user, x-other\"\t ".as_slice(),
        ] {
            verify_policy(&[policy], private, false).await;
        }
    }
}

#[tokio::test]
async fn quoted_extension_values_do_not_create_cache_directives() {
    for private in [false, true] {
        for policy in [
            b"extension=\"value, private, no-store, no-cache\"".as_slice(),
            b"extension=\"value\\\", no-store, private\"".as_slice(),
            b"extension=\"\x80, no-store, private\"".as_slice(),
        ] {
            verify_policy(&[policy], private, true).await;
        }
    }
}
