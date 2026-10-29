//! A JWK's optional `alg` must not prevent verification under an explicit policy.

#![cfg(feature = "oidc")]

use std::convert::Infallible;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::Bytes;
use http_body_util::Full;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use jsonwebtoken::{Algorithm, EncodingKey, Header, Validation};
use salvo_core::Depot;
use salvo_core::hyper::Response;
use salvo_core::hyper::server::conn::http1;
use salvo_core::hyper::service::service_fn;
use salvo_jwt_auth::oidc::OidcDecoder;
use salvo_jwt_auth::{JwtAuthDecoder, install_crypto_provider};
use serde_json::{Value, json};

const MODULUS: &[u8] = include_bytes!("fixtures/oidc-rsa-modulus.bin");
// Public test fixture only; never use this key outside tests.
const PRIVATE_KEY: &[u8] = include_bytes!("fixtures/oidc-rsa.pem");

struct ServerTask(tokio::task::JoinHandle<()>);

impl Drop for ServerTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn provider(jwk: Value, validation: Option<Validation>) -> (OidcDecoder, String, ServerTask) {
    let _ = install_crypto_provider();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let discovery = json!({"issuer": issuer, "jwks_uri": format!("{issuer}/jwks")}).to_string();
    let jwks = json!({"keys": [jwk]}).to_string();
    let server = ServerTask(tokio::spawn(async move {
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
                        .header("cache-control", "max-age=3600")
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
    let mut builder = OidcDecoder::builder(&issuer)
        .audience("client")
        .http_client(client);
    if let Some(validation) = validation {
        builder = builder.validation(validation);
    }
    let decoder = builder.build().await.unwrap();
    (decoder, issuer, server)
}

fn rsa_jwk() -> Value {
    json!({"kty":"RSA", "kid":"signing", "use":"sig", "key_ops":["verify"], "n":URL_SAFE_NO_PAD.encode(MODULUS), "e":"AQAB"})
}

fn token(issuer: &str, algorithm: Algorithm) -> String {
    let mut header = Header::new(algorithm);
    header.kid = Some("signing".into());
    jsonwebtoken::encode(
        &header,
        &json!({"iss":issuer,"aud":"client","sub":"user","exp":4_000_000_000u64}),
        &EncodingKey::from_rsa_pem(PRIVATE_KEY).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn rsa_jwks_without_alg_authenticates_with_configured_and_default_validation() {
    for validation in [Some(Validation::new(Algorithm::RS256)), None] {
        let (decoder, issuer, _server) = provider(rsa_jwk(), validation).await;
        let decoded = decoder
            .decode::<Value>(&token(&issuer, Algorithm::RS256), &mut Depot::new())
            .await
            .expect("a standards-compliant JWK without alg should authenticate");
        assert_eq!(decoded.claims["sub"], "user");
    }
}

#[tokio::test]
async fn missing_alg_never_expands_the_configured_allowlist() {
    let (decoder, issuer, _server) =
        provider(rsa_jwk(), Some(Validation::new(Algorithm::RS256))).await;
    assert!(
        decoder
            .decode::<Value>(&token(&issuer, Algorithm::RS512), &mut Depot::new())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn declared_alg_and_key_purpose_are_still_enforced() {
    for (field, value) in [
        ("alg", json!("RS512")),
        ("use", json!("enc")),
        ("key_ops", json!(["encrypt"])),
    ] {
        let mut jwk = rsa_jwk();
        jwk[field] = value;
        let (decoder, issuer, _server) =
            provider(jwk, Some(Validation::new(Algorithm::RS256))).await;
        assert!(
            decoder
                .decode::<Value>(&token(&issuer, Algorithm::RS256), &mut Depot::new())
                .await
                .is_err(),
            "{field}"
        );
    }
}
