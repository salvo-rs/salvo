//! Audit evidence: a successful max-age=0 revalidation is rejected for its own use.
#![cfg(feature = "oidc")]
use std::convert::Infallible;
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

async fn check(policy: &'static str) -> bool {
    let _ = install_crypto_provider();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let discovery = json!({"issuer":issuer,"jwks_uri":format!("{issuer}/jwks")}).to_string();
    let jwks =
        json!({"keys":[{"kty":"oct","kid":"signing","alg":"HS256","k":"c2VjcmV0"}]}).to_string();
    let _server = ServerTask(tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let (discovery, jwks) = (discovery.clone(), jwks.clone());
            tokio::spawn(async move {
                let service = service_fn(move |req: salvo_core::hyper::Request<_>| {
                    let body = if req.uri().path() == "/jwks" {
                        &jwks
                    } else {
                        &discovery
                    };
                    let response = Response::builder()
                        .header("cache-control", policy)
                        .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
                        .unwrap();
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
        .unwrap()
        .revalidation_interval(Duration::ZERO);
    let mut header = Header::default();
    header.kid = Some("signing".into());
    let token = jsonwebtoken::encode(
        &header,
        &json!({"iss":issuer,"aud":"client","exp":4_000_000_000u64}),
        &EncodingKey::from_base64_secret("c2VjcmV0").unwrap(),
    )
    .unwrap();
    decoder
        .decode::<Value>(&token, &mut Depot::new())
        .await
        .is_ok()
}

#[tokio::test]
async fn zero_lifetime_uses_the_directly_revalidated_response() {
    assert!(
        check("no-cache").await,
        "control: direct per-use validation succeeds"
    );
    assert!(
        check("max-age=0, must-revalidate").await,
        "the initiating request may use the validated key"
    );
    assert!(check("max-age=0, stale-while-revalidate=0, stale-if-error=0").await);
}
