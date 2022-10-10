use std::io::{Error as IoError, ErrorKind, Result as IoResult};

use base64::URL_SAFE_NO_PAD;
use http::{Method, Uri};
use hyper_util::client::connect::HttpConnector;
// use hyper_util::client::Client;
use hyper_rustls::HttpsConnector;
use ring::digest::{digest, Digest, SHA256};
use serde::{de::DeserializeOwned, Serialize};

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
    #[inline]
    fn base64(jwk: Option<Jwk>, kid: Option<&'a str>, nonce: &'a str, url: &'a str) -> IoResult<String> {
        let protected = Self {
            alg: "ES256",
            jwk,
            kid,
            nonce,
            url,
        };
        let protected = serde_json::to_vec(&protected)
            .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to encode jwt: {}", e)))?;
        Ok(base64::encode_config(protected, URL_SAFE_NO_PAD))
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
            x: base64::encode_config(x, URL_SAFE_NO_PAD),
            y: base64::encode_config(y, URL_SAFE_NO_PAD),
        }
    }

    #[inline]
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
            .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to encode jwt: {}", e)))?;
        let hash = sha256(&json);
        Ok(base64::encode_config(hash, URL_SAFE_NO_PAD))
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
    cli: &Client<HttpsConnector<HttpConnector>>,
    key_pair: &KeyPair,
    kid: Option<&str>,
    nonce: &str,
    uri: &str,
    payload: Option<impl Serialize>,
) -> IoResult<hyper::Response<hyper::body::Recv>> {
    let jwk = match kid {
        None => Some(Jwk::new(key_pair)),
        Some(_) => None,
    };
    let protected = Protected::base64(jwk, kid, nonce, uri)?;
    let payload = match payload {
        Some(payload) => serde_json::to_vec(&payload)
            .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to encode payload: {}", e)))?,
        None => Vec::new(),
    };
    let payload = base64::encode_config(payload, URL_SAFE_NO_PAD);
    let combined = format!("{}.{}", &protected, &payload);
    let signature = base64::encode_config(key_pair.sign(combined.as_bytes())?, URL_SAFE_NO_PAD);
    let body = serde_json::to_vec(&Body {
        protected,
        payload,
        signature,
    })
    .unwrap();
    let req = hyper::Request::builder()
        .method(Method::POST)
        .uri(uri.parse::<Uri>().unwrap())
        .header("content-type", "application/jose+json")
        .body(body.into())
        .unwrap();

    tracing::debug!(uri = %uri, "http request");

    let resp = cli
        .request(req)
        .await
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to send http request: {}", e)))?;
    if !resp.status().is_success() {
        return Err(IoError::new(
            ErrorKind::Other,
            format!("unexpected status code: status = {}", resp.status()),
        ));
    }
    Ok(resp)
}
#[inline]
pub(crate) async fn request_json<T, R>(
    cli: &Client<HttpsConnector<HttpConnector>>,
    key_pair: &KeyPair,
    kid: Option<&str>,
    nonce: &str,
    url: &str,
    payload: Option<T>,
) -> IoResult<R>
where
    T: Serialize,
    R: DeserializeOwned,
{
    let res = request(cli, key_pair, kid, nonce, url, payload).await?;

    let data = hyper::body::to_bytes(res)
        .await
        .map_err(|_| IoError::new(ErrorKind::Other, "failed to read response"))?;
    serde_json::from_slice(data.as_ref()).map_err(|e| IoError::new(ErrorKind::Other, format!("bad response: {}", e)))
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
