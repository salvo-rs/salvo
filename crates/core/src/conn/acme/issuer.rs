use std::io::Result as IoResult;
use std::sync::Arc;
use std::time::Duration;

use rcgen::{CertificateParams, CustomExtension, DistinguishedName, KeyPair};
use tokio_rustls::rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::{crypto::ring::sign::any_ecdsa_type, sign::CertifiedKey};

use super::cache::AcmeCache;
use super::client::AcmeClient;
use super::config::AcmeConfig;
use super::resolver::ResolveServerCert;
use super::{jose, ChallengeType};

use crate::Error;

pub(crate) async fn issue_cert(
    client: &mut AcmeClient,
    config: &AcmeConfig,
    resolver: &ResolveServerCert,
) -> crate::Result<()> {
    tracing::debug!("issue certificate");
    let order_res = client.new_order(&config.domains).await?;
    // trigger challenge
    let mut valid = false;
    for i in 1..5 {
        let mut all_valid = true;
        for auth_url in &order_res.authorizations {
            let res = client.fetch_authorization(auth_url).await?;
            if res.status == "valid" {
                continue;
            }
            all_valid = false;
            if res.status == "pending" {
                let challenge = res.find_challenge(config.challenge_type)?;
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
                        let auth_key = gen_acme_cert(&res.identifier.value, key_authorization_sha256.as_ref())?;
                        resolver
                            .acme_keys
                            .write()
                            .insert(res.identifier.value.to_string(), Arc::new(auth_key));
                    }
                }
                client
                    .trigger_challenge(&res.identifier.value, config.challenge_type, &challenge.url)
                    .await?;
            } else if res.status == "invalid" {
                tracing::error!(response = ?res, "unable to authorize");
                return Err(Error::other(format!(
                    "unable to authorize `{}`: {}",
                    res.identifier.value,
                    res.error.as_ref().map(|problem| &*problem.detail).unwrap_or("unknown")
                )));
            }
        }
        if all_valid {
            valid = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(i * 10)).await;
    }
    if !valid {
        return Err(Error::other("authorization failed too many times"));
    }
    // send csr
    let mut params = CertificateParams::new(config.domains.clone())
        .map_err(|e| Error::other(format!("crate certificate params failed: {}", e)))?;
    params.distinguished_name = DistinguishedName::new();

    let key_pair = KeyPair::generate().map_err(|e| Error::other(format!("generate key pair failed: {}", e)))?;

    let csr = params
        .serialize_request(&key_pair)
        .map_err(|e| Error::other(format!("failed to serialize request der {}", e)))?;
 
    let pk = any_ecdsa_type(&PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        key_pair.serialize_der(),
    )))
    .expect("serialize private key der failed");

    let order_res = client.send_csr(&order_res.finalize, csr.der()).await?;
    if order_res.status == "invalid" {
        return Err(Error::other(format!(
            "failed to request certificate: {}",
            order_res
                .error
                .as_ref()
                .map(|problem| &*problem.detail)
                .unwrap_or("unknown")
        )));
    }
    if order_res.status != "valid" {
        return Err(Error::other(format!(
            "failed to request certificate: unexpected status `{}`",
            order_res.status
        )));
    }
    // download certificate
    let cert_pem = client
        .obtain_certificate(
            order_res
                .certificate
                .as_ref()
                .ok_or_else(|| Error::other("invalid response: missing `certificate` url"))?,
        )
        .await?
        .as_ref()
        .to_vec();
    let key_pem = key_pair.serialize_pem();
    let cert_chain = rustls_pemfile::certs(&mut cert_pem.as_slice()).collect::<IoResult<Vec<_>>>()?;
    let cert_key = CertifiedKey::new(cert_chain, pk);
    *resolver.cert.write() = Some(Arc::new(cert_key));
    tracing::debug!("certificate obtained");
    if let Some(cache_path) = &config.cache_path {
        cache_path
            .write_key(&config.directory_name, &config.domains, key_pem.as_bytes())
            .await?;
        cache_path
            .write_cert(&config.directory_name, &config.domains, &cert_pem)
            .await?;
    }
    Ok(())
}

fn gen_acme_cert(domain: &str, acme_hash: &[u8]) -> crate::Result<CertifiedKey> {
    let key_pair = KeyPair::generate().map_err(|e| Error::other(format!("generate key pair failed: {}", e)))?;

    let mut params = CertificateParams::new(vec![domain.to_string()])
        .map_err(|e| Error::other(format!("create certificate params failed: {}", e)))?;
    params.custom_extensions = vec![CustomExtension::new_acme_identifier(acme_hash)];
    let cert = params
        .self_signed(&key_pair)
        .map_err(|_| Error::other("failed to generate acme certificate"))?;
    let pk = any_ecdsa_type(&PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        key_pair.serialize_der(),
    )))
    .expect("serialize private key der failed");
    Ok(CertifiedKey::new(
        vec![cert.der().clone()],
        pk,
    ))
}
