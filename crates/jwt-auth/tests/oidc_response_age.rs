//! A shared cache's Age must consume the JWKS freshness budget.

#![cfg(feature = "oidc")]

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use jsonwebtoken::{EncodingKey, Header};
use salvo_core::Depot;
use salvo_core::hyper::Response;
use salvo_core::hyper::server::conn::http1;
use salvo_core::hyper::service::service_fn;
use salvo_jwt_auth::oidc::OidcDecoder;
use salvo_jwt_auth::{JwtAuthDecoder, install_crypto_provider};
use serde_json::{Value, json};

struct ServerTask(tokio::task::JoinHandle<()>);

impl Drop for ServerTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn aged_jwks_refreshes_before_reusing_a_revoked_signing_key() {
    let _ = install_crypto_provider();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let discovery = json!({"issuer":issuer,"jwks_uri":format!("{issuer}/jwks")}).to_string();
    let revoked = Arc::new(AtomicBool::new(false));
    let fetches = Arc::new(AtomicUsize::new(0));
    let (server_revoked, server_fetches) = (revoked.clone(), fetches.clone());
    let _server = ServerTask(tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let (revoked, fetches, discovery) = (
                server_revoked.clone(),
                server_fetches.clone(),
                discovery.clone(),
            );
            tokio::spawn(async move {
                let service = service_fn(move |req: salvo_core::hyper::Request<_>| {
                    let response = if req.uri().path() == "/jwks" {
                        fetches.fetch_add(1, Ordering::SeqCst);
                        let body = if revoked.load(Ordering::SeqCst) {
                            json!({"keys":[]})
                        } else {
                            json!({"keys":[{"kty":"oct","kid":"signing","alg":"HS256","k":"c2VjcmV0"}]})
                        };
                        Response::builder()
                            .header(
                                "cache-control",
                                "max-age=3600, stale-while-revalidate=0, stale-if-error=0",
                            )
                            .header("age", "3599")
                            .body(Full::new(Bytes::from(body.to_string())))
                            .unwrap()
                    } else {
                        Response::new(Full::new(Bytes::copy_from_slice(discovery.as_bytes())))
                    };
                    async { Ok::<_, Infallible>(response) }
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    }));

    #[cfg(feature = "aws-lc-rs")]
    let crypto = rustls::crypto::aws_lc_rs::default_provider();
    #[cfg(all(not(feature = "aws-lc-rs"), feature = "ring"))]
    let crypto = rustls::crypto::ring::default_provider();
    let tls = rustls::ClientConfig::builder_with_provider(crypto.into())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
    let connector = HttpsConnectorBuilder::new()
        .with_tls_config(tls)
        .https_or_http()
        .enable_http1()
        .build();
    let client = Client::builder(TokioExecutor::new()).build(connector);
    let decoder = OidcDecoder::builder(&issuer)
        .audience("client")
        .allow_symmetric_jwks(true)
        .http_client(client)
        .build()
        .await
        .unwrap();
    let mut header = Header::default();
    header.kid = Some("signing".into());
    let token = jsonwebtoken::encode(
        &header,
        &json!({"iss":issuer,"aud":"client","exp":4_000_000_000u64}),
        &EncodingKey::from_secret(b"secret"),
    )
    .unwrap();
    assert!(
        decoder
            .decode::<Value>(&token, &mut Depot::new())
            .await
            .is_ok()
    );
    assert_eq!(fetches.load(Ordering::SeqCst), 1);

    revoked.store(true, Ordering::SeqCst);
    // The shared cache consumed 3,599 of the allowed 3,600 seconds already.
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert!(
        decoder
            .decode::<Value>(&token, &mut Depot::new())
            .await
            .is_err(),
        "Age must not give an already cached JWKS a fresh one-hour lifetime"
    );
    assert_eq!(fetches.load(Ordering::SeqCst), 2);
}
