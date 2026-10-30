//! Failed token persistence must not expose a token or execute the application.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_csrf::{Csrf, CsrfCipher, CsrfDepotExt, CsrfRotationPolicy, CsrfStore, HeaderFinder};

struct Cipher;
impl CsrfCipher for Cipher {
    fn verify(&self, token: &str, proof: &str) -> bool {
        proof == format!("proof-{token}")
    }
    fn generate(&self) -> (String, String) {
        ("new".into(), "proof-new".into())
    }
}

struct Store {
    current: Option<(String, String)>,
    fail_save: bool,
    saves: Arc<AtomicUsize>,
}
impl CsrfStore for Store {
    type Error = std::io::Error;

    async fn load<C: CsrfCipher>(
        &self,
        _req: &mut Request,
        _depot: &mut Depot,
        _cipher: &C,
    ) -> Option<(String, String)> {
        self.current.clone()
    }
    async fn save(
        &self,
        _req: &mut Request,
        _depot: &mut Depot,
        _res: &mut Response,
        _token: &str,
        _proof: &str,
    ) -> Result<(), Self::Error> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        if self.fail_save {
            Err(std::io::Error::other("store unavailable"))
        } else {
            Ok(())
        }
    }
}

struct Application(Arc<AtomicUsize>);
#[async_trait]
impl Handler for Application {
    async fn handle(
        &self,
        _req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        self.0.fetch_add(1, Ordering::SeqCst);
        res.render(depot.csrf_token().unwrap().to_owned());
    }
}

#[tokio::test]
async fn token_save_failure_stops_initial_issuance_and_rotation() {
    for (existing, post) in [(false, false), (true, false), (true, true)] {
        let called = Arc::new(AtomicUsize::new(0));
        let saves = Arc::new(AtomicUsize::new(0));
        let service = Service::new(
            Router::new()
                .hoop(
                    Csrf::new(
                        Cipher,
                        Store {
                            current: existing.then(|| ("old".into(), "proof-old".into())),
                            fail_save: true,
                            saves: saves.clone(),
                        },
                        HeaderFinder::new("x-csrf-token"),
                    )
                    .rotation_policy(CsrfRotationPolicy::PerRequest),
                )
                .goal(Application(called.clone())),
        );
        let request = if post {
            TestClient::post("http://localhost/").add_header("x-csrf-token", "old", true)
        } else {
            TestClient::get("http://localhost/")
        };
        let response = request.send(&service).await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::INTERNAL_SERVER_ERROR)
        );
        assert_eq!(saves.load(Ordering::SeqCst), 1);
        assert_eq!(called.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn successful_token_save_still_exposes_the_saved_token() {
    for (existing, post) in [(false, false), (true, false), (true, true)] {
        let called = Arc::new(AtomicUsize::new(0));
        let saves = Arc::new(AtomicUsize::new(0));
        let service = Service::new(
            Router::new()
                .hoop(
                    Csrf::new(
                        Cipher,
                        Store {
                            current: existing.then(|| ("old".into(), "proof-old".into())),
                            fail_save: false,
                            saves: saves.clone(),
                        },
                        HeaderFinder::new("x-csrf-token"),
                    )
                    .rotation_policy(CsrfRotationPolicy::PerRequest),
                )
                .goal(Application(called.clone())),
        );
        let request = if post {
            TestClient::post("http://localhost/").add_header("x-csrf-token", "old", true)
        } else {
            TestClient::get("http://localhost/")
        };
        let mut response = request.send(&service).await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "new");
        assert_eq!(saves.load(Ordering::SeqCst), 1);
        assert_eq!(called.load(Ordering::SeqCst), 1);
    }
}

#[cfg(feature = "session-store")]
#[tokio::test]
async fn missing_session_handler_is_a_server_error() {
    let called = Arc::new(AtomicUsize::new(0));
    let service = Service::new(
        Router::new()
            .hoop(Csrf::new(
                Cipher,
                salvo_csrf::SessionStore::new(),
                HeaderFinder::new("x-csrf-token"),
            ))
            .goal(Application(called.clone())),
    );
    let response = TestClient::get("http://localhost/").send(&service).await;
    assert_eq!(
        response.status_code,
        Some(StatusCode::INTERNAL_SERVER_ERROR)
    );
    assert_eq!(called.load(Ordering::SeqCst), 0);
}
