use std::io::{Error as IoError, Result as IoResult};

use super::client::HyperClient;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http_body_util::{BodyExt, Full};
use hyper::{Method, body::Incoming as HyperBody};
use ring::digest::{Digest, SHA256, digest};
use serde::{Serialize, de::DeserializeOwned};

use crate::Error;
use crate::conn::acme::key_pair::KeyPair;

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
            .map_err(|e| IoError::other(format!("failed to encode jwt: {}", e)))?;
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
            .map_err(|e| IoError::other(format!("failed to encode jwt: {}", e)))?;
        Ok(URL_SAFE_NO_PAD.encode(sha256(json)))
    }
}

#[inline]
fn sha256(data: impl AsRef<[u8]>) -> Digest {
    digest(&SHA256, data.as_ref())
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
            .map_err(|e| IoError::other(format!("failed to encode payload: {}", e)))?,
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
        .map_err(|e| IoError::other(format!("failed to build http request: {}", e)))?;

    let res = client
        .request(req)
        .await
        .map_err(|e| IoError::other(format!("failed to send http request: {}", e)))?;
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
) -> crate::Result<R>
where
    T: Serialize + Send,
    R: DeserializeOwned,
{
    let res = request(cli, key_pair, kid, nonce, url, payload).await?;

    let data = res.into_body().collect().await?.to_bytes();
    serde_json::from_slice(&data)
        .map_err(|e| Error::other(format!("response is not a valid json: {}", e)))
}

#[inline]
pub(crate) fn key_authorization(key: &KeyPair, token: &str) -> IoResult<String> {
    let jwk = Jwk::new(key);
    let key_authorization = format!("{}.{}", token, jwk.thumb_sha256_base64()?);
    Ok(key_authorization)
}

#[inline]
pub(crate) fn key_authorization_sha256(key: &KeyPair, token: &str) -> IoResult<impl AsRef<[u8]>> {
    Ok(sha256(key_authorization(key, token)?.as_bytes()))
}
