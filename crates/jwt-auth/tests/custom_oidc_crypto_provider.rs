//! Application-selected OIDC rustls provider tests.

#[cfg(all(feature = "oidc", feature = "aws-lc-rs", feature = "ring"))]
#[tokio::test]
async fn preserves_an_explicitly_installed_rustls_provider() {
    use salvo_jwt_auth::oidc::DecoderBuilder;

    let provider = rustls::crypto::ring::default_provider();
    let expected_secure_random = provider.secure_random;
    provider
        .install_default()
        .expect("the integration test process should not have a rustls provider yet");

    let result = DecoderBuilder::new("https://")
        .audience("test-client")
        .build()
        .await;
    assert!(result.is_err());

    let provider = rustls::crypto::CryptoProvider::get_default()
        .expect("the application-selected rustls provider should remain installed");
    assert!(std::ptr::eq(provider.secure_random, expected_secure_random));
}
