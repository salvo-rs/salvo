//! Application-selected crypto provider tests.

#[cfg(all(feature = "aws-lc-rs", feature = "ring"))]
#[test]
fn preserves_an_explicitly_installed_provider() {
    use std::sync::atomic::{AtomicBool, Ordering};

    use jsonwebtoken::crypto::{CryptoProvider, JwkUtils, JwtSigner, JwtVerifier};
    use jsonwebtoken::errors::{ErrorKind, Result};
    use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header};
    use salvo_jwt_auth::ConstDecoder;

    static CUSTOM_PROVIDER_USED: AtomicBool = AtomicBool::new(false);

    fn signer_factory(_: &Algorithm, _: &EncodingKey) -> Result<Box<dyn JwtSigner>> {
        CUSTOM_PROVIDER_USED.store(true, Ordering::SeqCst);
        Err(ErrorKind::InvalidAlgorithm.into())
    }

    fn verifier_factory(_: &Algorithm, _: &DecodingKey) -> Result<Box<dyn JwtVerifier>> {
        Err(ErrorKind::InvalidAlgorithm.into())
    }

    static CUSTOM_PROVIDER: CryptoProvider = CryptoProvider {
        signer_factory,
        verifier_factory,
        jwk_utils: JwkUtils::new_unimplemented(),
    };

    CUSTOM_PROVIDER
        .install_default()
        .expect("the integration test process should not have a provider yet");

    let _decoder = ConstDecoder::from_secret(b"secret");
    let result = jsonwebtoken::encode(
        &Header::default(),
        &Claims { sub: "test" },
        &EncodingKey::from_secret(b"secret"),
    );

    assert!(result.is_err());
    assert!(CUSTOM_PROVIDER_USED.load(Ordering::SeqCst));
}

#[derive(serde::Serialize)]
struct Claims {
    sub: &'static str,
}
