use std::sync::Arc;

use rustls::pki_types::pem::PemObject;

use rustls::{
    ClientConfig, DigitallySignedStruct, Error, OtherError, RootCertStore, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::CryptoProvider,
    pki_types::{CertificateDer, ServerName, UnixTime},
};

/// SSL verification mode parsed from the `sslmode` query parameter.
///
/// Matches libpq's modes, with one notable difference: libpq falls back to
/// `~/.postgresql/root.crt` (or the system CA file) when `sslrootcert` is
/// not specified for `verify-ca` / `verify-full`. We don't implement that
/// file lookup, so both modes require an explicit `sslrootcert` parameter.
/// Use `sslrootcert=system` for the OS trust store with `verify-full`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SslVerifyMode {
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

pub(crate) fn build_client_config(
    mode: SslVerifyMode,
    sslrootcert: Option<&str>,
    sslcert: Option<&str>,
    sslkey: Option<&str>,
) -> Result<ClientConfig, toasty_core::Error> {
    let provider = match CryptoProvider::get_default() {
        Some(p) => p.clone(),
        None => {
            let provider = rustls::crypto::ring::default_provider();
            let _ = provider.install_default();
            CryptoProvider::get_default()
                .expect("just installed")
                .clone()
        }
    };

    let client_auth = load_client_auth(sslcert, sslkey)?;

    // sslrootcert=system -> platform verifier, enforce verify-full
    if sslrootcert == Some("system") {
        if mode != SslVerifyMode::VerifyFull {
            return Err(toasty_core::Error::invalid_connection_url(
                "sslrootcert=system requires sslmode=verify-full",
            ));
        }
        let verifier = platform_verifier(&provider)?;
        let builder = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(toasty_core::Error::driver_operation_failed)?
            .dangerous()
            .with_custom_certificate_verifier(verifier);
        return apply_client_auth(builder, client_auth);
    }

    let roots = match sslrootcert {
        Some(path) => Some(load_root_certs(path)?),
        None => None,
    };

    let verifier: Arc<dyn ServerCertVerifier> = match mode {
        SslVerifyMode::Disable => unreachable!("TLS should not be built for sslmode=disable"),

        SslVerifyMode::Prefer | SslVerifyMode::Require => Arc::new(CaOnlyVerifier {
            roots: roots.map(Arc::new),
            provider: provider.clone(),
        }),

        SslVerifyMode::VerifyCa => {
            let roots = roots.ok_or_else(|| {
                toasty_core::Error::invalid_connection_url("sslmode=verify-ca requires sslrootcert")
            })?;
            Arc::new(CaOnlyVerifier {
                roots: Some(Arc::new(roots)),
                provider: provider.clone(),
            })
        }

        SslVerifyMode::VerifyFull => {
            let roots = roots.ok_or_else(|| {
                toasty_core::Error::invalid_connection_url(
                    "sslmode=verify-full requires sslrootcert (use sslrootcert=system for OS trust store)",
                )
            })?;
            rustls::client::WebPkiServerVerifier::builder_with_provider(
                Arc::new(roots),
                provider.clone(),
            )
            .build()
            .map_err(toasty_core::Error::driver_operation_failed)?
        }
    };

    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(toasty_core::Error::driver_operation_failed)?
        .dangerous()
        .with_custom_certificate_verifier(verifier);

    apply_client_auth(builder, client_auth)
}

/// Verifies the server certificate chain against trusted roots but does NOT
/// check that the server hostname matches the certificate.
///
/// When `roots` is `None`, accepts any certificate (encryption only).
/// When `roots` is `Some`, uses `webpki::EndEntityCert::verify_for_usage()`
/// directly to validate the chain without hostname checking.
///
/// Used for sslmode=prefer/require (with optional sslrootcert) and
/// sslmode=verify-ca (requires sslrootcert).
#[derive(Debug)]
struct CaOnlyVerifier {
    roots: Option<Arc<RootCertStore>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for CaOnlyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if let Some(roots) = &self.roots {
            let cert = webpki::EndEntityCert::try_from(end_entity).map_err(pki_error)?;
            cert.verify_for_usage(
                self.provider.signature_verification_algorithms.all,
                &roots.roots,
                intermediates,
                now,
                webpki::KeyUsage::server_auth(),
                None,
                None,
            )
            .map_err(pki_error)?;
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn pki_error(e: webpki::Error) -> Error {
    use rustls::CertificateError;
    use webpki::Error::*;
    match e {
        BadDer | BadDerTime | TrailingData(_) => CertificateError::BadEncoding.into(),
        CertNotValidYet { time, not_before } => {
            CertificateError::NotValidYetContext { time, not_before }.into()
        }
        CertExpired { time, not_after } => {
            CertificateError::ExpiredContext { time, not_after }.into()
        }
        InvalidCertValidity => CertificateError::Expired.into(),
        UnknownIssuer => CertificateError::UnknownIssuer.into(),
        CertRevoked => CertificateError::Revoked.into(),
        InvalidSignatureForPublicKey => CertificateError::BadSignature.into(),
        #[allow(deprecated)]
        UnsupportedSignatureAlgorithm | UnsupportedSignatureAlgorithmForPublicKey => {
            CertificateError::UnsupportedSignatureAlgorithm.into()
        }
        _ => CertificateError::Other(OtherError(Arc::new(e))).into(),
    }
}

type ClientAuthData = (
    Vec<CertificateDer<'static>>,
    rustls::pki_types::PrivateKeyDer<'static>,
);

fn load_client_auth(
    sslcert: Option<&str>,
    sslkey: Option<&str>,
) -> Result<Option<ClientAuthData>, toasty_core::Error> {
    match (sslcert, sslkey) {
        (Some(cert_path), Some(key_path)) => {
            let cert_data =
                std::fs::read(cert_path).map_err(toasty_core::Error::driver_operation_failed)?;
            let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&cert_data)
                .collect::<Result<Vec<_>, _>>()
                .map_err(toasty_core::Error::driver_operation_failed)?;
            if certs.is_empty() {
                return Err(toasty_core::Error::invalid_connection_url(format!(
                    "no certificates found in sslcert file: {cert_path}"
                )));
            }

            let key_data =
                std::fs::read(key_path).map_err(toasty_core::Error::driver_operation_failed)?;
            let key = rustls::pki_types::PrivateKeyDer::from_pem_slice(&key_data)
                .map_err(toasty_core::Error::driver_operation_failed)?;

            Ok(Some((certs, key)))
        }
        (None, None) => Ok(None),
        (Some(_), None) => Err(toasty_core::Error::invalid_connection_url(
            "sslcert specified without sslkey",
        )),
        (None, Some(_)) => Err(toasty_core::Error::invalid_connection_url(
            "sslkey specified without sslcert",
        )),
    }
}

fn apply_client_auth(
    builder: rustls::ConfigBuilder<ClientConfig, rustls::client::WantsClientCert>,
    client_auth: Option<ClientAuthData>,
) -> Result<ClientConfig, toasty_core::Error> {
    match client_auth {
        Some((certs, key)) => builder
            .with_client_auth_cert(certs, key)
            .map_err(toasty_core::Error::driver_operation_failed),
        None => Ok(builder.with_no_client_auth()),
    }
}

fn platform_verifier(
    provider: &Arc<CryptoProvider>,
) -> Result<Arc<dyn ServerCertVerifier>, toasty_core::Error> {
    Ok(Arc::new(
        rustls_platform_verifier::Verifier::new(provider.clone())
            .map_err(toasty_core::Error::driver_operation_failed)?,
    ))
}

fn load_root_certs(path: &str) -> Result<RootCertStore, toasty_core::Error> {
    let pem_data = std::fs::read(path).map_err(toasty_core::Error::driver_operation_failed)?;
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&pem_data)
        .collect::<Result<Vec<_>, _>>()
        .map_err(toasty_core::Error::driver_operation_failed)?;

    let mut store = RootCertStore::empty();
    for cert in certs {
        store
            .add(cert)
            .map_err(toasty_core::Error::driver_operation_failed)?;
    }

    if store.is_empty() {
        return Err(toasty_core::Error::invalid_connection_url(format!(
            "no certificates found in sslrootcert file: {path}"
        )));
    }

    Ok(store)
}

#[cfg(test)]
mod pem_tests {
    use super::*;

    fn write_file(dir: &tempfile::TempDir, name: &str, contents: &str) -> String {
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        path.to_str().unwrap().to_owned()
    }

    fn certificate() -> (String, String) {
        let generated = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
        (generated.cert.pem(), generated.signing_key.serialize_pem())
    }

    #[test]
    fn valid_root_and_client_identity_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let (certificate, key) = certificate();
        let cert_path = write_file(&dir, "certificate.pem", &certificate);
        let key_path = write_file(&dir, "key.pem", &key);
        assert_eq!(load_root_certs(&cert_path).unwrap().len(), 1);
        let (certs, _) = load_client_auth(Some(&cert_path), Some(&key_path))
            .unwrap()
            .unwrap();
        assert_eq!(certs.len(), 1);
        assert!(
            build_client_config(
                SslVerifyMode::Require,
                None,
                Some(&cert_path),
                Some(&key_path)
            )
            .is_ok()
        );
    }

    #[test]
    fn empty_malformed_and_non_certificate_roots_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (_, key) = certificate();
        for contents in [
            "",
            "not a certificate",
            key.as_str(),
            "-----BEGIN CERTIFICATE-----\n!invalid!\n-----END CERTIFICATE-----",
            "-----BEGIN CERTIFICATE-----\naGVsbG8=\n-----END CERTIFICATE-----",
        ] {
            let path = write_file(&dir, "root.pem", contents);
            assert!(load_root_certs(&path).is_err());
        }
    }

    #[test]
    fn client_auth_requires_both_files() {
        assert!(load_client_auth(None, None).unwrap().is_none());
        assert!(load_client_auth(Some("certificate.pem"), None).is_err());
        assert!(load_client_auth(None, Some("key.pem")).is_err());
    }

    #[test]
    fn invalid_and_missing_client_keys_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let (certificate, _) = certificate();
        let cert_path = write_file(&dir, "certificate.pem", &certificate);
        for key in [
            "",
            "not a private key",
            certificate.as_str(),
            "-----BEGIN PRIVATE KEY-----\n!invalid!\n-----END PRIVATE KEY-----",
            "-----BEGIN PRIVATE KEY-----\naGVsbG8=\n-----END PRIVATE KEY-----",
        ] {
            let key_path = write_file(&dir, "key.pem", key);
            assert!(
                build_client_config(
                    SslVerifyMode::Require,
                    None,
                    Some(&cert_path),
                    Some(&key_path)
                )
                .is_err()
            );
        }
        assert!(load_client_auth(Some(&cert_path), Some("nonexistent-key.pem")).is_err());
    }

    #[test]
    fn invalid_client_certificates_and_mismatched_keys_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let (cert, key) = certificate();
        let key_path = write_file(&dir, "key.pem", &key);
        for contents in [
            "",
            "not a certificate",
            key.as_str(),
            "-----BEGIN CERTIFICATE-----\n!invalid!\n-----END CERTIFICATE-----",
            "-----BEGIN CERTIFICATE-----\naGVsbG8=\n-----END CERTIFICATE-----",
        ] {
            let cert_path = write_file(&dir, "certificate.pem", contents);
            assert!(
                build_client_config(
                    SslVerifyMode::Require,
                    None,
                    Some(&cert_path),
                    Some(&key_path)
                )
                .is_err()
            );
        }
        let cert_path = write_file(&dir, "certificate.pem", &cert);
        let (_, wrong_key) = certificate();
        let wrong_key_path = write_file(&dir, "wrong-key.pem", &wrong_key);
        assert!(
            build_client_config(
                SslVerifyMode::Require,
                None,
                Some(&cert_path),
                Some(&wrong_key_path)
            )
            .is_err()
        );
    }
}
