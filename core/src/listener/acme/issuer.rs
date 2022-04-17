use std::io::{self, Error as IoError, ErrorKind, Result as IoResult};
use std::sync::Arc;
use std::time::Duration;

use rcgen::{Certificate, CertificateParams, CustomExtension, DistinguishedName, PKCS_ECDSA_P256_SHA256};
use tokio_rustls::rustls::sign::{any_ecdsa_type, CertifiedKey};
use tokio_rustls::rustls::PrivateKey;

use super::cache::AcmeCache;
use super::client::AcmeClient;
use super::config::AcmeConfig;
use super::resolver::ResolveServerCert;
use super::{jose, ChallengeType, WELL_KNOWN_PATH};

async fn check_before_issue(config: &AcmeConfig) -> IoResult<()> {
    let fake_token: String = (0..16).map(|_| fastrand::alphanumeric()).collect();
    for domain in &config.domains {
        let url = match config.challenge_type {
            ChallengeType::Http01 => {
                format!("http://{}{}/{}", domain, WELL_KNOWN_PATH, fake_token)
            }
            ChallengeType::TlsAlpn01 => {
                format!("https://{}{}/{}", domain, WELL_KNOWN_PATH, fake_token)
            }
        };

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        let body_bytes = client
            .get(url)
            .send()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?
            .bytes()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        if &body_bytes != fake_token.as_bytes() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "token is not equal, origin: {}  getted: {}",
                    fake_token,
                    String::from_utf8_lossy(&body_bytes)
                ),
            ));
        }
    }
    Ok(())
}

pub(crate) async fn issue_cert(
    client: &mut AcmeClient,
    config: &AcmeConfig,
    resolver: &ResolveServerCert,
) -> IoResult<()> {
    tracing::debug!("check before issue certificate");
    check_before_issue(config).await?;
    tracing::debug!("issue certificate");
    let order_resp = client.new_order(&config.domains).await?;
    // trigger challenge
    let mut valid = false;
    for i in 1..5 {
        let mut all_valid = true;
        for auth_url in &order_resp.authorizations {
            let resp = client.fetch_authorization(auth_url).await?;
            if resp.status == "valid" {
                continue;
            }
            all_valid = false;
            if resp.status == "pending" {
                let challenge = resp.find_challenge(config.challenge_type)?;
                match config.challenge_type {
                    ChallengeType::Http01 => {
                        if let Some(keys) = &config.keys_for_http01 {
                            let key_authorization = jose::key_authorization(&config.key_pair, &challenge.token)?;
                            let mut keys = keys.write();
                            keys.insert(challenge.token.to_string(), key_authorization);
                        }
                    }
                    ChallengeType::TlsAlpn01 => {
                        let key_authorization_sha256 =
                            jose::key_authorization_sha256(&config.key_pair, &challenge.token)?;
                        let auth_key = gen_acme_cert(&resp.identifier.value, key_authorization_sha256.as_ref())?;
                        resolver
                            .acme_keys
                            .write()
                            .insert(resp.identifier.value.to_string(), Arc::new(auth_key));
                    }
                }
                client
                    .trigger_challenge(&resp.identifier.value, config.challenge_type, &challenge.url)
                    .await?;
            } else if resp.status == "invalid" {
                return Err(IoError::new(
                    ErrorKind::Other,
                    format!(
                        "unable to authorize `{}`: {}",
                        resp.identifier.value,
                        resp.error.as_ref().map(|problem| &*problem.detail).unwrap_or("unknown")
                    ),
                ));
            }
        }
        if all_valid {
            valid = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(i * 10)).await;
    }
    if !valid {
        return Err(IoError::new(ErrorKind::Other, "authorization failed too many times"));
    }
    // send csr
    let mut params = CertificateParams::new(config.domains.clone());
    params.distinguished_name = DistinguishedName::new();
    params.alg = &PKCS_ECDSA_P256_SHA256;
    let cert = Certificate::from_params(params)
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed create certificate request: {}", e)))?;
    let pk = any_ecdsa_type(&PrivateKey(cert.serialize_private_key_der())).unwrap();
    let csr = cert
        .serialize_request_der()
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to serialize request der {}", e)))?;
    let order_resp = client.send_csr(&order_resp.finalize, &csr).await?;
    if order_resp.status == "invalid" {
        return Err(IoError::new(
            ErrorKind::Other,
            format!(
                "failed to request certificate: {}",
                order_resp
                    .error
                    .as_ref()
                    .map(|problem| &*problem.detail)
                    .unwrap_or("unknown")
            ),
        ));
    }
    if order_resp.status != "valid" {
        return Err(IoError::new(
            ErrorKind::Other,
            format!(
                "failed to request certificate: unexpected status `{}`",
                order_resp.status
            ),
        ));
    }
    // download certificate
    let cert_pem = client
        .obtain_certificate(
            &*order_resp
                .certificate
                .as_ref()
                .ok_or_else(|| IoError::new(ErrorKind::Other, "invalid response: missing `certificate` url"))?,
        )
        .await?
        .as_ref()
        .to_vec();
    let pkey_pem = cert.serialize_private_key_pem();
    let cert_chain = rustls_pemfile::certs(&mut cert_pem.as_slice())
        .map_err(|e| IoError::new(ErrorKind::Other, format!("invalid pem: {}", e)))?
        .into_iter()
        .map(tokio_rustls::rustls::Certificate)
        .collect();
    let cert_key = CertifiedKey::new(cert_chain, pk);
    *resolver.cert.write() = Some(Arc::new(cert_key));
    tracing::debug!("certificate obtained");
    if let Some(cache_path) = &config.cache_path {
        cache_path
            .write_pkey_pem(&config.directory_name, &config.domains, pkey_pem.as_bytes())
            .await?;
        cache_path
            .write_cert_pem(&config.directory_name, &config.domains, &cert_pem)
            .await?;
    }
    Ok(())
}

fn gen_acme_cert(domain: &str, acme_hash: &[u8]) -> IoResult<CertifiedKey> {
    let mut params = CertificateParams::new(vec![domain.to_string()]);
    params.alg = &PKCS_ECDSA_P256_SHA256;
    params.custom_extensions = vec![CustomExtension::new_acme_identifier(acme_hash)];
    let cert = Certificate::from_params(params)
        .map_err(|_| IoError::new(ErrorKind::Other, "failed to generate acme certificate"))?;
    let key = any_ecdsa_type(&PrivateKey(cert.serialize_private_key_der())).unwrap();
    Ok(CertifiedKey::new(
        vec![tokio_rustls::rustls::Certificate(cert.serialize_der().map_err(
            |_| IoError::new(ErrorKind::Other, "failed to serialize acme certificate"),
        )?)],
        key,
    ))
}
