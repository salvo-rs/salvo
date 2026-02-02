use std::io::{Error as IoError, Result as IoResult};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http_body_util::{BodyExt, Full};
use hyper::Method;
use hyper::body::Incoming as HyperBody;
use salvo_core::{Error as CoreError, Result as CoreResult};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::client::HyperClient;
use crate::key_pair::KeyPair;

#[derive(Serialize)]
struct Protected<'a> {
    alg: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    jwk: Option<Jwk>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kid: Option<&'a str>,
    nonce: &'a str,
    url: &'a str,
}

impl<'a> Protected<'a> {
    fn base64(
        jwk: Option<Jwk>,
        kid: Option<&'a str>,
        nonce: &'a str,
        url: &'a str,
    ) -> IoResult<String> {
        let protected = Self {
            alg: "ES256",
            jwk,
            kid,
            nonce,
            url,
        };
        let protected = serde_json::to_vec(&protected)
            .map_err(|e| IoError::other(format!("failed to encode jwt: {e}")))?;
        Ok(URL_SAFE_NO_PAD.encode(protected))
    }
}

#[derive(Serialize)]
struct Jwk {
    alg: &'static str,
    crv: &'static str,
    kty: &'static str,
    #[serde(rename = "use")]
    u: &'static str,
    x: String,
    y: String,
}

impl Jwk {
    #[inline]
    fn new(key: &KeyPair) -> Self {
        let (x, y) = key.public_key()[1..].split_at(32);
        Self {
            alg: "ES256",
            crv: "P-256",
            kty: "EC",
            u: "sig",
            x: URL_SAFE_NO_PAD.encode(x),
            y: URL_SAFE_NO_PAD.encode(y),
        }
    }

    fn thumb_sha256_base64(&self) -> IoResult<String> {
        #[derive(Serialize)]
        struct JwkThumb<'a> {
            crv: &'a str,
            kty: &'a str,
            x: &'a str,
            y: &'a str,
        }

        let jwk_thumb = JwkThumb {
            crv: self.crv,
            kty: self.kty,
            x: &self.x,
            y: &self.y,
        };
        let json = serde_json::to_vec(&jwk_thumb)
            .map_err(|e| IoError::other(format!("failed to encode jwt: {e}")))?;
        Ok(URL_SAFE_NO_PAD.encode(sha256(json)))
    }
}

#[cfg(any(feature = "aws-lc-rs", not(feature = "ring")))]
#[inline]
fn sha256(data: impl AsRef<[u8]>) -> Vec<u8> {
    aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, data.as_ref())
        .as_ref()
        .to_vec()
}

#[cfg(all(not(feature = "aws-lc-rs"), feature = "ring"))]
#[inline]
fn sha256(data: impl AsRef<[u8]>) -> Vec<u8> {
    ring::digest::digest(&ring::digest::SHA256, data.as_ref())
        .as_ref()
        .to_vec()
}

#[derive(Serialize)]
struct Body {
    protected: String,
    payload: String,
    signature: String,
}

pub(crate) async fn request(
    client: &HyperClient,
    key_pair: &KeyPair,
    kid: Option<&str>,
    nonce: &str,
    uri: &str,
    payload: Option<impl Serialize + Send>,
) -> IoResult<hyper::Response<HyperBody>> {
    let jwk = match kid {
        None => Some(Jwk::new(key_pair)),
        Some(_) => None,
    };
    let protected = Protected::base64(jwk, kid, nonce, uri)?;
    let payload = match payload {
        Some(payload) => serde_json::to_vec(&payload)
            .map_err(|e| IoError::other(format!("failed to encode payload: {e}")))?,
        None => Vec::new(),
    };
    let payload = URL_SAFE_NO_PAD.encode(payload);
    let combined = format!("{}.{}", &protected, &payload);
    let signature = URL_SAFE_NO_PAD.encode(key_pair.sign(combined.as_bytes())?);
    let body = serde_json::to_vec(&Body {
        protected,
        payload,
        signature,
    })
    .map_err(IoError::other)?;

    let req = hyper::Request::builder()
        .header("content-type", "application/jose+json")
        .method(Method::POST)
        .uri(uri)
        .body(Full::from(body))
        .map_err(|e| IoError::other(format!("failed to build http request: {e}")))?;

    let res = client
        .request(req)
        .await
        .map_err(|e| IoError::other(format!("failed to send http request: {e}")))?;
    if !res.status().is_success() {
        return Err(IoError::other(format!(
            "unexpected status code: status = {}",
            res.status()
        )));
    }
    Ok(res)
}
pub(crate) async fn request_json<T, R>(
    cli: &HyperClient,
    key_pair: &KeyPair,
    kid: Option<&str>,
    nonce: &str,
    url: &str,
    payload: Option<T>,
) -> CoreResult<R>
where
    T: Serialize + Send,
    R: DeserializeOwned,
{
    let res = request(cli, key_pair, kid, nonce, url, payload).await?;

    let data = res.into_body().collect().await?.to_bytes();
    serde_json::from_slice(&data)
        .map_err(|e| CoreError::other(format!("response is not a valid json: {e}")))
}

#[inline]
pub(crate) fn key_authorization(key: &KeyPair, token: &str) -> IoResult<String> {
    let jwk = Jwk::new(key);
    let key_authorization = format!("{}.{}", token, jwk.thumb_sha256_base64()?);
    Ok(key_authorization)
}

#[cfg(any(feature = "aws-lc-rs", feature = "ring"))]
#[inline]
pub(crate) fn key_authorization_sha256(key: &KeyPair, token: &str) -> IoResult<impl AsRef<[u8]>> {
    Ok(sha256(key_authorization(key, token)?.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwk_new() {
        let key_pair = KeyPair::generate().unwrap();
        let jwk = Jwk::new(&key_pair);

        assert_eq!(jwk.alg, "ES256");
        assert_eq!(jwk.crv, "P-256");
        assert_eq!(jwk.kty, "EC");
        assert_eq!(jwk.u, "sig");
        // x and y should be base64url encoded
        assert!(!jwk.x.is_empty());
        assert!(!jwk.y.is_empty());
    }

    #[test]
    fn test_jwk_thumb_sha256_base64() {
        let key_pair = KeyPair::generate().unwrap();
        let jwk = Jwk::new(&key_pair);

        let thumb = jwk.thumb_sha256_base64();
        assert!(thumb.is_ok());

        let thumb_str = thumb.unwrap();
        // SHA256 produces 32 bytes, base64url encoded should be ~43 characters
        assert!(!thumb_str.is_empty());
        // Base64url should not contain + or /
        assert!(!thumb_str.contains('+'));
        assert!(!thumb_str.contains('/'));
    }

    #[test]
    fn test_jwk_deterministic() {
        let key_pair = KeyPair::generate().unwrap();
        let jwk1 = Jwk::new(&key_pair);
        let jwk2 = Jwk::new(&key_pair);

        // Same key pair should produce same JWK
        assert_eq!(jwk1.x, jwk2.x);
        assert_eq!(jwk1.y, jwk2.y);
    }

    #[test]
    fn test_protected_base64() {
        let key_pair = KeyPair::generate().unwrap();
        let jwk = Jwk::new(&key_pair);
        let nonce = "test_nonce";
        let url = "https://example.com/acme";

        let result = Protected::base64(Some(jwk), None, nonce, url);
        assert!(result.is_ok());

        let encoded = result.unwrap();
        // Should be base64url encoded
        assert!(!encoded.is_empty());
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
    }

    #[test]
    fn test_protected_base64_with_kid() {
        let nonce = "test_nonce";
        let url = "https://example.com/acme";
        let kid = "https://example.com/acme/acct/12345";

        let result = Protected::base64(None, Some(kid), nonce, url);
        assert!(result.is_ok());

        let encoded = result.unwrap();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_key_authorization() {
        let key_pair = KeyPair::generate().unwrap();
        let token = "test_token_12345";

        let result = key_authorization(&key_pair, token);
        assert!(result.is_ok());

        let auth = result.unwrap();
        // Key authorization format: token.thumbprint
        assert!(auth.contains('.'));
        assert!(auth.starts_with(token));
    }

    #[test]
    fn test_key_authorization_deterministic() {
        let key_pair = KeyPair::generate().unwrap();
        let token = "test_token";

        let auth1 = key_authorization(&key_pair, token).unwrap();
        let auth2 = key_authorization(&key_pair, token).unwrap();

        // Same key and token should produce same authorization
        assert_eq!(auth1, auth2);
    }

    #[test]
    fn test_key_authorization_sha256() {
        let key_pair = KeyPair::generate().unwrap();
        let token = "test_token";

        let result = key_authorization_sha256(&key_pair, token);
        assert!(result.is_ok());

        let hash = result.unwrap();
        let hash_bytes: &[u8] = hash.as_ref();
        // SHA256 produces 32 bytes
        assert_eq!(hash_bytes.len(), 32);
    }

    #[test]
    fn test_sha256() {
        let data = b"test data";
        let hash = sha256(data);

        // SHA256 produces 32 bytes
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_sha256_deterministic() {
        let data = b"test data";
        let hash1 = sha256(data);
        let hash2 = sha256(data);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_sha256_different_input() {
        let hash1 = sha256(b"data1");
        let hash2 = sha256(b"data2");

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_body_serialization() {
        let body = Body {
            protected: "protected_data".to_string(),
            payload: "payload_data".to_string(),
            signature: "signature_data".to_string(),
        };

        let json = serde_json::to_string(&body);
        assert!(json.is_ok());

        let json_str = json.unwrap();
        assert!(json_str.contains("protected"));
        assert!(json_str.contains("payload"));
        assert!(json_str.contains("signature"));
    }
}
