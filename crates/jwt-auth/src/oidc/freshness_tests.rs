use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use hyper_util::rt::TokioIo;
use salvo_core::hyper::Response;
use salvo_core::hyper::server::conn::http1;
use salvo_core::hyper::service::service_fn;
use serde_json::json;
use tokio::sync::{Notify, Semaphore};

use super::*;

struct OriginState {
    fetches: AtomicUsize,
    revoked: AtomicBool,
    failed: AtomicBool,
    stalled: AtomicBool,
    entered: Notify,
    gate: Semaphore,
    policy: std::sync::Mutex<&'static str>,
}
impl Default for OriginState {
    fn default() -> Self {
        Self {
            fetches: AtomicUsize::new(0),
            revoked: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            stalled: AtomicBool::new(false),
            entered: Notify::new(),
            gate: Semaphore::new(0),
            policy: std::sync::Mutex::new(""),
        }
    }
}

struct Origin {
    issuer: String,
    state: Arc<OriginState>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Origin {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Origin {
    async fn start(policy: &'static str, date: Option<HeaderValue>) -> Self {
        let _ = crate::install_crypto_provider();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let discovery = json!({"issuer":issuer,"jwks_uri":format!("{issuer}/jwks")}).to_string();
        let state = Arc::new(OriginState {
            policy: std::sync::Mutex::new(policy),
            ..Default::default()
        });
        let shared = state.clone();
        let task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.unwrap();
                        let (state, discovery, date) = (shared.clone(), discovery.clone(), date.clone());
                        connections.spawn(async move {
                            let service = service_fn(move |req: salvo_core::hyper::Request<_>| {
                                let (state, discovery, date) = (state.clone(), discovery.clone(), date.clone());
                                async move {
                                    let jwks = req.uri().path() == "/jwks";
                                    if jwks {
                                        assert_eq!(req.headers()[CACHE_CONTROL], "no-cache");
                                        state.fetches.fetch_add(1, Ordering::SeqCst);
                                        if state.stalled.load(Ordering::SeqCst) {
                                            state.entered.notify_one();
                                            state.gate.acquire().await.unwrap().forget();
                                        }
                                    }
                                    let body = if !jwks { discovery }
                                        else if state.revoked.load(Ordering::SeqCst) { json!({"keys":[]}).to_string() }
                                        else { json!({"keys":[{"kty":"oct","kid":"cached","alg":"HS256","k":"c2VjcmV0"}]}).to_string() };
                                    let mut response = Response::builder()
                                        .status(if jwks && state.failed.load(Ordering::SeqCst) { 503 } else { 200 })
                                        .header(CACHE_CONTROL, *state.policy.lock().unwrap());
                                    if let Some(date) = date { response = response.header("date", date); }
                                    Ok::<_, Infallible>(response.body(Full::new(Bytes::from(body))).unwrap())
                                }
                            });
                            let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
                        });
                    }
                    Some(_) = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Self {
            issuer,
            state,
            task,
        }
    }

    async fn decoder(&self) -> OidcDecoder {
        OidcDecoder::builder(&self.issuer)
            .audience("client")
            .allow_symmetric_jwks(true)
            .http_client(super::tests::test_http_client())
            .build()
            .await
            .unwrap()
            .revalidation_interval(Duration::ZERO)
    }

    fn token(&self) -> String {
        let mut header = jsonwebtoken::Header::new(Algorithm::HS256);
        header.kid = Some("cached".into());
        jsonwebtoken::encode(
            &header,
            &json!({"iss":self.issuer,"aud":"client","exp":current_time()+3600}),
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap()
    }
}

async fn validates(decoder: &OidcDecoder, token: &str) -> bool {
    decoder
        .decode::<serde_json::Value>(token, &mut Depot::new())
        .await
        .is_ok()
}

#[test]
fn initial_age_uses_date_age_and_transfer_with_injected_time() {
    let received = UNIX_EPOCH + Duration::from_secs(1_000);
    let mut headers = HeaderMap::new();
    headers.typed_insert(Date::from(UNIX_EPOCH + Duration::from_secs(900)));
    headers.insert(AGE, HeaderValue::from_static("20"));
    assert_eq!(
        corrected_initial_age(&headers, received, Duration::from_secs(5)),
        Duration::from_secs(100)
    );
    headers.insert(AGE, HeaderValue::from_static("150"));
    assert_eq!(
        corrected_initial_age(&headers, received, Duration::from_secs(5)),
        Duration::from_secs(155)
    );
    headers.remove(AGE);
    // Future Date / clock rollback never create a negative age or subtract transfer time.
    assert_eq!(
        corrected_initial_age(&headers, UNIX_EPOCH, Duration::from_secs(5)),
        Duration::from_secs(5)
    );
}

#[tokio::test]
async fn no_cache_and_no_store_revalidate_and_reject_revoked_or_unavailable_keys() {
    timeout(Duration::from_secs(5), async {
        for directive in [
            "no-cache, max-age=3600, stale-if-error=3600",
            "no-store, max-age=3600, stale-while-revalidate=3600",
        ] {
            let origin = Origin::start(directive, None).await;
            let decoder = origin.decoder().await;
            let token = origin.token();
            if directive.starts_with("no-store") {
                assert!(decoder.cache.read().await.jwks.keys.is_empty());
            }
            assert!(validates(&decoder, &token).await);
            assert_eq!(origin.state.fetches.load(Ordering::SeqCst), 2);
            origin.state.failed.store(true, Ordering::SeqCst);
            assert!(!validates(&decoder, &token).await);
            origin.state.failed.store(false, Ordering::SeqCst);
            assert!(validates(&decoder, &token).await);
            origin.state.revoked.store(true, Ordering::SeqCst);
            assert!(!validates(&decoder, &token).await);
            if directive.starts_with("no-store") {
                let cache = decoder.cache.read().await;
                assert!(cache.jwks.keys.is_empty());
                assert!(cache.get_key("cached").is_none());
            }
        }
    })
    .await
    .expect("local revalidation test timed out");
}

#[tokio::test]
async fn must_revalidate_never_uses_stale_if_error() {
    timeout(Duration::from_secs(5), async {
        let origin = Origin::start("max-age=1, must-revalidate, stale-if-error=3600", None).await;
        let decoder = origin.decoder().await;
        let token = origin.token();
        assert!(validates(&decoder, &token).await);
        origin.state.failed.store(true, Ordering::SeqCst);
        decoder.cache_state.set_response_age(Duration::from_secs(2));
        assert!(!validates(&decoder, &token).await);
        assert_eq!(origin.state.fetches.load(Ordering::SeqCst), 2);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn old_date_is_not_given_a_new_fresh_lifetime() {
    timeout(Duration::from_secs(5), async {
        let mut headers = HeaderMap::new();
        headers.typed_insert(Date::from(SystemTime::now() - Duration::from_secs(900)));
        let date = headers["date"].clone();
        let origin = Origin::start(
            "max-age=600, stale-if-error=0, stale-while-revalidate=0",
            Some(date),
        )
        .await;
        let decoder = origin.decoder().await;
        assert!(!validates(&decoder, &origin.token()).await);
        assert!(decoder.cache_state.age() >= Duration::from_secs(900));
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn per_use_validation_has_an_explicit_fail_closed_budget() {
    timeout(Duration::from_secs(5), async {
        let origin = Origin::start("no-cache", None).await;
        let decoder = origin
            .decoder()
            .await
            .revalidation_interval(Duration::from_secs(3600));
        let token = origin.token();
        assert!(validates(&decoder, &token).await);
        assert!(!validates(&decoder, &token).await);
        assert_eq!(origin.state.fetches.load(Ordering::SeqCst), 2);
        assert!(validates(&decoder.revalidation_interval(Duration::ZERO), &token).await);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn cancelled_per_use_validation_releases_its_refresh_claim() {
    timeout(Duration::from_secs(5), async {
        let origin = Origin::start("no-store", None).await;
        let decoder = origin.decoder().await;
        origin.state.stalled.store(true, Ordering::SeqCst);
        let mut lookup = Box::pin(decoder.get_kid_retry("cached"));
        tokio::select! {
            _ = origin.state.entered.notified() => {},
            _ = &mut lookup => panic!("the response should remain gated"),
        }
        assert!(decoder.cache_state.is_revalidating());
        drop(lookup);
        assert!(!decoder.cache_state.is_revalidating());
        origin.state.stalled.store(false, Ordering::SeqCst);
        origin.state.gate.add_permits(1);
        assert!(validates(&decoder, &origin.token()).await);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_validated_policy_change_controls_subsequent_reuse() {
    timeout(Duration::from_secs(5), async {
        let origin = Origin::start("no-store", None).await;
        let decoder = origin.decoder().await;
        let token = origin.token();
        assert!(decoder.cache.read().await.jwks.keys.is_empty());
        *origin.state.policy.lock().unwrap() = "max-age=600";
        assert!(validates(&decoder, &token).await);
        assert!(validates(&decoder, &token).await);
        assert_eq!(origin.state.fetches.load(Ordering::SeqCst), 2);
        // A policy switch must apply even when the key bytes are identical.
        *origin.state.policy.lock().unwrap() = "no-store";
        decoder.update_cache().await.unwrap();
        assert!(decoder.cache.read().await.jwks.keys.is_empty());
        origin.state.revoked.store(true, Ordering::SeqCst);
        assert!(!validates(&decoder, &token).await);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn explicit_zero_without_stale_allowances_never_has_a_fresh_window() {
    timeout(Duration::from_secs(5), async {
        let origin = Origin::start(
            "max-age=0, stale-while-revalidate=0, stale-if-error=0",
            None,
        )
        .await;
        let decoder = origin.decoder().await;
        assert!(!validates(&decoder, &origin.token()).await);
        assert_eq!(
            decoder.cache.read().await.cache_policy.max_age,
            Duration::ZERO
        );
    })
    .await
    .unwrap();
}
