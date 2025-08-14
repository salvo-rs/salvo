use std::sync::Arc;

use base64::engine::{Engine, general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::Uri;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};

use super::{Challenge, Problem};
use super::{ChallengeType, jose, key_pair::KeyPair};
use super::{Directory, Identifier};

use crate::Error;

pub(super) type HyperClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>;

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

impl FetchAuthorizationResponse {
    pub(crate) fn find_challenge(&self, ctype: ChallengeType) -> crate::Result<&Challenge> {
        self.challenges
            .iter()
            .find(|c| c.kind == ctype.to_string())
            .ok_or_else(|| Error::other(format!("unable to find `{ctype}` challenge")))
    }
}

pub(crate) struct AcmeClient {
    pub(crate) client: HyperClient,
    pub(crate) directory: Directory,
    pub(crate) key_pair: Arc<KeyPair>,
    pub(crate) contacts: Vec<String>,
    pub(crate) kid: Option<String>,
}

impl AcmeClient {
    pub(crate) async fn new(
        directory_url: &str,
        key_pair: Arc<KeyPair>,
        contacts: Vec<String>,
    ) -> crate::Result<Self> {
        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .expect("no native root CA certificates found")
            .https_only()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(https);
        let directory = get_directory(&client, directory_url).await?;
        Ok(Self {
            client,
            directory,
            key_pair,
            contacts,
            kid: None,
        })
    }

    pub(crate) async fn new_order(
        &mut self,
        domains: &[String],
    ) -> crate::Result<NewOrderResponse> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct NewOrderRequest {
            identifiers: Vec<Identifier>,
        }

        let kid = if let Some(kid) = &self.kid {
            kid
        } else {
            // create account
            let kid = create_acme_account(
                &self.client,
                &self.directory,
                &self.key_pair,
                self.contacts.clone(),
            )
            .await?;
            self.kid = Some(kid);
            self.kid.as_ref().expect("kid should not none")
        };
        tracing::debug!(kid = kid.as_str(), "new order request");

        let nonce = get_nonce(&self.client, &self.directory.new_nonce).await?;
        let res: NewOrderResponse = jose::request_json(
            &self.client,
            &self.key_pair,
            Some(kid),
            &nonce,
            &self.directory.new_order,
            Some(NewOrderRequest {
                identifiers: domains
                    .iter()
                    .map(|domain| Identifier {
                        kind: "dns".to_owned(),
                        value: domain.to_string(),
                    })
                    .collect(),
            }),
        )
        .await?;

        tracing::debug!(status = res.status.as_str(), "order created");
        Ok(res)
    }

    pub(crate) async fn fetch_authorization(
        &self,
        auth_url: &str,
    ) -> crate::Result<FetchAuthorizationResponse> {
        tracing::debug!(auth_url, "fetch authorization");

        let nonce = get_nonce(&self.client, &self.directory.new_nonce).await?;
        let res: FetchAuthorizationResponse = jose::request_json(
            &self.client,
            &self.key_pair,
            self.kid.as_deref(),
            &nonce,
            auth_url,
            None::<()>,
        )
        .await?;

        tracing::debug!(
            identifier = ?res.identifier,
            status = res.status.as_str(),
            "authorization response",
        );

        Ok(res)
    }

    pub(crate) async fn trigger_challenge(
        &self,
        domain: &str,
        challenge_type: ChallengeType,
        url: &str,
    ) -> crate::Result<()> {
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

    pub(crate) async fn send_csr(&self, url: &str, csr: &[u8]) -> crate::Result<NewOrderResponse> {
        tracing::debug!(url, "send certificate request");

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
                csr: URL_SAFE_NO_PAD.encode(csr),
            }),
        )
        .await
    }

    pub(crate) async fn obtain_certificate(&self, url: &str) -> crate::Result<Bytes> {
        tracing::debug!(url, "send certificate request");

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
        Ok(res.into_body().collect().await?.to_bytes())
    }
}

async fn get_directory(client: &HyperClient, directory_url: &str) -> crate::Result<Directory> {
    tracing::debug!("loading directory");
    let directory_url = directory_url
        .parse::<Uri>()
        .map_err(|e| Error::other(format!("failed to parse directory dir: {e}")))?;
    let res = client
        .get(directory_url)
        .await
        .map_err(|e| Error::other(format!("failed to load directory: {e}")))?;

    if !res.status().is_success() {
        return Err(Error::other(format!(
            "failed to load directory: status = {}",
            res.status()
        )));
    }

    let data = res
        .into_body()
        .collect()
        .await
        .map_err(|e| Error::other(format!("failed to load body: {e}")))?
        .to_bytes();
    let directory = serde_json::from_slice::<Directory>(&data)
        .map_err(|e| Error::other(format!("failed to load directory: {e}")))?;

    tracing::debug!(
        new_nonce = ?directory.new_nonce,
        new_account = ?directory.new_account,
        new_order = ?directory.new_order,
        "directory loaded",
    );
    Ok(directory)
}

async fn get_nonce(client: &HyperClient, nonce_url: &str) -> crate::Result<String> {
    tracing::debug!("creating nonce");

    let res = client
        .get(nonce_url.parse::<Uri>()?)
        .await
        .map_err(|e| Error::other(format!("failed to get nonce: {e}")))?;

    if !res.status().is_success() {
        return Err(Error::other(format!(
            "failed to load directory: status = {}",
            res.status()
        )));
    }

    let nonce = res
        .headers()
        .get("replay-nonce")
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
        .unwrap_or_default();

    tracing::debug!(nonce = nonce.as_str(), "nonce created");
    Ok(nonce)
}

async fn create_acme_account(
    client: &HyperClient,
    directory: &Directory,
    key_pair: &KeyPair,
    contacts: Vec<String>,
) -> crate::Result<String> {
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
        .get("location")
        .ok_or_else(|| Error::other("unable to get account id"))?
        .to_str()
        .map(|s| s.to_owned())
        .map_err(|_| Error::other("unable to get account id"));

    tracing::debug!(?kid, "account created");
    kid
}
