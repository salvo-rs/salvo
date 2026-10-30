//! Session-backed CSRF tokens must be reissued after their proof becomes invalid.

#![cfg(all(feature = "session-store", feature = "hmac-cipher"))]

use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_csrf::{Csrf, CsrfDepotExt, HeaderFinder, HmacCipher};
use salvo_session::{MemoryStore, SessionHandler};

#[handler]
async fn token(depot: &mut Depot) -> String {
    depot.csrf_token().unwrap().to_owned()
}

#[handler]
async fn accepted() -> &'static str {
    "accepted"
}

fn service(store: MemoryStore, cipher: HmacCipher) -> Service {
    Service::new(
        Router::new()
            .hoop(SessionHandler::builder(store, &[42; 64]).build().unwrap())
            .hoop(Csrf::new(
                cipher,
                salvo_csrf::SessionStore::new(),
                HeaderFinder::new("x-csrf-token"),
            ))
            .get(token)
            .post(accepted),
    )
}

#[tokio::test]
async fn safe_request_recovers_session_token_after_cipher_configuration_changes() {
    for (old_cipher, new_cipher) in [
        (HmacCipher::new([1; 32]), HmacCipher::new([2; 32])),
        (
            HmacCipher::new([1; 32]),
            HmacCipher::new([1; 32]).token_size(16),
        ),
    ] {
        let store = MemoryStore::new();
        let old_service = service(store.clone(), old_cipher);
        let mut first = TestClient::get("http://localhost/")
            .send(&old_service)
            .await;
        let old_cookie = format!(
            "salvo.session.id={}",
            first.cookie("salvo.session.id").unwrap().value()
        );
        let old_token = first.take_string().await.unwrap();

        let new_service = service(store, new_cipher);
        // An old proof must not authorize an unsafe request under the new key.
        let rejected = TestClient::post("http://localhost/")
            .add_header("cookie", old_cookie.clone(), true)
            .add_header("x-csrf-token", old_token.clone(), true)
            .send(&new_service)
            .await;
        assert_eq!(rejected.status_code, Some(StatusCode::FORBIDDEN));

        let mut refreshed = TestClient::get("http://localhost/")
            .add_header("cookie", old_cookie.clone(), true)
            .send(&new_service)
            .await;
        assert_eq!(refreshed.status_code, Some(StatusCode::OK));
        let cookie = refreshed
            .cookie("salvo.session.id")
            .map(|cookie| format!("salvo.session.id={}", cookie.value()))
            .unwrap_or(old_cookie);
        let new_token = refreshed.take_string().await.unwrap();
        assert_ne!(new_token, old_token, "safe request reused an invalid proof");

        let response = TestClient::post("http://localhost/")
            .add_header("cookie", cookie.clone(), true)
            .add_header("x-csrf-token", new_token.clone(), true)
            .send(&new_service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));

        let mut stable = TestClient::get("http://localhost/")
            .add_header("cookie", cookie, true)
            .send(&new_service)
            .await;
        assert_eq!(stable.take_string().await.unwrap(), new_token);
    }
}
