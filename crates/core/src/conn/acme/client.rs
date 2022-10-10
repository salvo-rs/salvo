use std::{
    io::{Error as IoError, ErrorKind, Result as IoResult},
    sync::Arc,
};

use base64::URL_SAFE_NO_PAD;
use bytes::Bytes;
use http::header;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::connect::HttpConnector;
// use hyper_util::client::Client;
use serde::{Deserialize, Serialize};

use super::{Challenge, Problem};

use super::{jose, key_pair::KeyPair, ChallengeType};
use super::{Directory, Identifier};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewOrderResponse {
    pub(crate) status: String,
    pub(crate) authorizations: Vec<String>,
    pub(crate) error: Option<Problem>,
    pub(crate) finalize: String,
    pub(crate) certificate: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FetchAuthorizationResponse {
    pub(crate) identifier: Identifier,
    pub(crate) status: String,
    pub(crate) challenges: Vec<Challenge>,
    pub(crate) error: Option<Problem>,
}

pub(crate) struct AcmeClient {
    pub(crate) client: Client<HttpsConnector<HttpConnector>>,
    pub(crate) directory: Directory,
    pub(crate) key_pair: Arc<KeyPair>,
    pub(crate) contacts: Vec<String>,
    pub(crate) kid: Option<String>,
}

impl AcmeClient {
    #[inline]
    pub(crate) async fn try_new(directory_url: &str, key_pair: Arc<KeyPair>, contacts: Vec<String>) -> IoResult<Self> {
        let client = Client::builder().build(
            HttpsConnectorBuilder::new()
                .with_native_roots()
                .https_or_http()
                .enable_http1()
                .build(),
        );
        let directory = get_directory(&client, directory_url).await?;
        Ok(Self {
            client,
            directory,
            key_pair,
            contacts,
            kid: None,
        })
    }

    pub(crate) async fn new_order(&mut self, domains: &[String]) -> IoResult<NewOrderResponse> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct NewOrderRequest {
            identifiers: Vec<Identifier>,
        }

        impl FetchAuthorizationResponse {
            pub(crate) fn find_challenge(&self, ctype: ChallengeType) -> IoResult<&Challenge> {
                self.challenges
                    .iter()
                    .find(|c| c.kind == ctype.to_string())
                    .ok_or_else(|| IoError::new(ErrorKind::Other, format!("unable to find `{}` challenge", ctype)))
            }
        }

        let kid = match &self.kid {
            Some(kid) => kid,
            None => {
                // create account
                let kid =
                    create_acme_account(&self.client, &self.directory, &self.key_pair, self.contacts.clone()).await?;
                self.kid = Some(kid);
                self.kid.as_ref().unwrap()
            }
        };
        tracing::debug!(kid = kid.as_str(), "new order request");

        let nonce = get_nonce(&self.client, &self.directory.new_nonce).await?;
        let resp: NewOrderResponse = jose::request_json(
            &self.client,
            &self.key_pair,
            Some(kid),
            &nonce,
            &self.directory.new_order,
            Some(NewOrderRequest {
                identifiers: domains
                    .iter()
                    .map(|domain| Identifier {
                        kind: "dns".to_string(),
                        value: domain.to_string(),
                    })
                    .collect(),
            }),
        )
        .await?;

        tracing::debug!(status = resp.status.as_str(), "order created");
        Ok(resp)
    }

    #[inline]
    pub(crate) async fn fetch_authorization(&self, auth_url: &str) -> IoResult<FetchAuthorizationResponse> {
        tracing::debug!(auth_uri = %auth_url, "fetch authorization");

        let nonce = get_nonce(&self.client, &self.directory.new_nonce).await?;
        let resp: FetchAuthorizationResponse = jose::request_json(
            &self.client,
            &self.key_pair,
            self.kid.as_deref(),
            &nonce,
            auth_url,
            None::<()>,
        )
        .await?;

        tracing::debug!(
            identifier = ?resp.identifier,
            status = resp.status.as_str(),
            "authorization response",
        );

        Ok(resp)
    }

    #[inline]
    pub(crate) async fn trigger_challenge(
        &self,
        domain: &str,
        challenge_type: ChallengeType,
        url: &str,
    ) -> IoResult<()> {
        tracing::debug!(
            auth_uri = %url,
            domain = domain,
            challenge_type = %challenge_type,
            "trigger challenge",
        );

        let nonce = get_nonce(&self.client, &self.directory.new_nonce).await?;
        jose::request(
            &self.client,
            &self.key_pair,
            self.kid.as_deref(),
            &nonce,
            url,
            Some(serde_json::json!({})),
        )
        .await?;

        Ok(())
    }

    #[inline]
    pub(crate) async fn send_csr(&self, url: &str, csr: &[u8]) -> IoResult<NewOrderResponse> {
        tracing::debug!(url = %url, "send certificate request");

        #[derive(Debug, Serialize)]
        #[serde(rename_all = "camelCase")]
        struct CsrRequest {
            csr: String,
        }

        let nonce = get_nonce(&self.client, &self.directory.new_nonce).await?;
        jose::request_json(
            &self.client,
            &self.key_pair,
            self.kid.as_deref(),
            &nonce,
            url,
            Some(CsrRequest {
                csr: base64::encode_config(csr, URL_SAFE_NO_PAD),
            }),
        )
        .await
    }

    #[inline]
    pub(crate) async fn obtain_certificate(&self, url: &str) -> IoResult<Bytes> {
        tracing::debug!(url = %url, "send certificate request");

        let nonce = get_nonce(&self.client, &self.directory.new_nonce).await?;
        let res = jose::request(
            &self.client,
            &self.key_pair,
            self.kid.as_deref(),
            &nonce,
            url,
            None::<()>,
        )
        .await?;
        hyper::body::to_bytes(res)
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to download certificate: {}", e)))
    }
}

async fn get_directory(client: &Client<HttpsConnector<HttpConnector>>, directory_url: &str) -> IoResult<Directory> {
    tracing::debug!("loading directory");

    let directory_url = directory_url
        .parse()
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to parse url: {}", e)))?;
    let res = client
        .get(directory_url)
        .await
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to load directory: {}", e)))?;

    if !res.status().is_success() {
        return Err(IoError::new(
            ErrorKind::Other,
            format!("failed to load directory: status = {}", res.status()),
        ));
    }

    let directory = serde_json::from_slice::<Directory>(
        hyper::body::to_bytes(res)
            .await
            .map_err(|_| IoError::new(ErrorKind::Other, "failed to read response"))?
            .as_ref(),
    )
    .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to load directory: {}", e)))?;

    tracing::debug!(
        new_nonce = ?directory.new_nonce,
        new_account = ?directory.new_account,
        new_order = ?directory.new_order,
        "directory loaded",
    );
    Ok(directory)
}

async fn get_nonce(client: &Client<HttpsConnector<HttpConnector>>, nonce_url: &str) -> IoResult<String> {
    tracing::debug!("creating nonce");

    let new_nonce = nonce_url
        .parse()
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to parse url: {}", e)))?;
    let resp = client
        .get(new_nonce)
        .await
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to get nonce: {}", e)))?;

    if !resp.status().is_success() {
        return Err(IoError::new(
            ErrorKind::Other,
            format!("failed to load directory: status = {}", resp.status()),
        ));
    }

    let nonce = resp
        .headers()
        .get("replay-nonce")
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
        .unwrap_or_default();

    tracing::debug!(nonce = nonce.as_str(), "nonce created");
    Ok(nonce)
}

async fn create_acme_account(
    client: &Client<HttpsConnector<HttpConnector>>,
    directory: &Directory,
    key_pair: &KeyPair,
    contacts: Vec<String>,
) -> IoResult<String> {
    tracing::debug!("creating acme account");

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct NewAccountRequest {
        only_return_existing: bool,
        terms_of_service_agreed: bool,
        contacts: Vec<String>,
    }

    let nonce = get_nonce(client, &directory.new_nonce).await?;
    let res = jose::request(
        client,
        key_pair,
        None,
        &nonce,
        &directory.new_account,
        Some(NewAccountRequest {
            only_return_existing: false,
            terms_of_service_agreed: true,
            contacts,
        }),
    )
    .await?;
    let kid = res
        .headers()
        .get(header::LOCATION)
        .ok_or_else(|| IoError::new(ErrorKind::Other, "unable to get account id"))?
        .to_str()
        .map(|s| s.to_owned())
        .map_err(|_| IoError::new(ErrorKind::Other, "unable to get account id"));

    tracing::debug!(kid = ?kid, "account created");
    kid
}
