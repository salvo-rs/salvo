//! OIDC rustls provider feature interaction tests.

#[cfg(all(feature = "oidc", feature = "aws-lc-rs", feature = "ring"))]
#[tokio::test]
async fn both_provider_features_select_aws_lc_for_oidc() {
    use salvo_jwt_auth::oidc::DecoderBuilder;

    let expected_secure_random = rustls::crypto::aws_lc_rs::default_provider().secure_random;

    // The invalid issuer lets the build fail immediately after constructing
    // the default HTTPS client, without making a network request.
    let result = DecoderBuilder::new("https://")
        .audience("test-client")
        .build()
        .await;
    assert!(result.is_err());

    let provider = rustls::crypto::CryptoProvider::get_default()
        .expect("OIDC should install a deterministic rustls provider");
    assert!(std::ptr::eq(provider.secure_random, expected_secure_random));
}
