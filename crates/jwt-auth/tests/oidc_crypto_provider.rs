//! OIDC rustls provider feature interaction tests.

#[cfg(all(feature = "oidc", feature = "aws-lc-rs", feature = "ring"))]
#[tokio::test]
async fn default_oidc_client_does_not_install_a_process_provider() {
    use salvo_jwt_auth::oidc::DecoderBuilder;

    assert!(rustls::crypto::CryptoProvider::get_default().is_none());

    // The invalid issuer lets the build fail immediately after constructing
    // the default HTTPS client, without making a network request.
    let result = DecoderBuilder::new("https://")
        .audience("test-client")
        .build()
        .await;
    assert!(result.is_err());

    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
}
