use crate::net::CertificateInput;

use crate::Error;
use rustls::client::WebPkiServerVerifier as WebPkiVerifier;
use rustls::client::danger::HandshakeSignatureValid;
use rustls::client::danger::ServerCertVerifier;
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::ServerName;
use rustls::pki_types::{CertificateDer, TrustAnchor, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore};
use rustls::pki_types::pem::PemObject;
use std::sync::Arc;

pub async fn configure_tls_connector(
    accept_invalid_certs: bool,
    accept_invalid_hostnames: bool,
    root_cert_path: Option<&CertificateInput>,
) -> Result<crate::rt::TlsConnector, Error> {
    let config = ClientConfig::builder().dangerous();
    let config = if accept_invalid_certs {
        config
            .with_custom_certificate_verifier(Arc::new(DummyTlsVerifier))
            .with_no_client_auth()
    } else {
        let mut cert_store = RootCertStore::empty();
        cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().map(|ta| TrustAnchor {
            subject: ta.subject.clone(),
            subject_public_key_info: ta.subject_public_key_info.clone(),
            name_constraints: ta.name_constraints.clone(),
        }));
        if let Some(ca) = root_cert_path {
            let data = ca.data().await?;
            let mut certificate_count = 0;
            for cert_result in CertificateDer::pem_slice_iter(&data) {
                let cert =
                    cert_result.map_err(|_| Error::from(format!("Invalid certificate {}", ca)))?;
                cert_store
                    .add(cert)
                    .map_err(|err| Error::from(err.to_string()))?;
                certificate_count += 1;
            }
            if certificate_count == 0 {
                return Err(Error::from(format!("No certificates found in {}", ca)));
            }
        }
        if accept_invalid_hostnames {
            let verifier = WebPkiVerifier::builder(Arc::new(cert_store))
                .build()
                .map_err(|e| Error::from(e.to_string()))?;
            config
                .with_custom_certificate_verifier(Arc::new(NoHostnameTlsVerifier { verifier }))
                .with_no_client_auth()
        } else {
            config
                .cfg
                .with_root_certificates(cert_store)
                .with_no_client_auth()
        }
    };

    Ok(Arc::new(config).into())
}

#[derive(Debug)]
struct DummyTlsVerifier;

impl ServerCertVerifier for DummyTlsVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[derive(Debug)]
pub struct NoHostnameTlsVerifier {
    verifier: Arc<WebPkiVerifier>,
}

impl ServerCertVerifier for NoHostnameTlsVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        match self.verifier.verify_server_cert(
            _end_entity,
            _intermediates,
            _server_name,
            _ocsp,
            _now,
        ) {
            Ok(res) => Ok(res),
            Err(e) => {
                return match e {
                    rustls::Error::InvalidCertificate(reason) => match reason {
                        // rustls reports the structured `...Context` variant for
                        // name mismatches on newer versions.
                        CertificateError::NotValidForName
                        | CertificateError::NotValidForNameContext { .. } => {
                            Ok(rustls::client::danger::ServerCertVerified::assertion())
                        }
                        reason => Err(rustls::Error::InvalidCertificate(reason)),
                    },
                    _ => Err(e),
                };
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
