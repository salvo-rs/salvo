//! Crypto provider feature interaction tests.

#[cfg(all(feature = "aws-lc-rs", feature = "ring"))]
#[test]
fn explicit_initialization_precedes_direct_jsonwebtoken_use() {
    use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
    use salvo_jwt_auth::{ConstDecoder, decode, install_crypto_provider};
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Serialize)]
    struct Claims {
        sub: String,
        exp: u64,
    }

    install_crypto_provider()
        .expect("the provider must be installed before direct jsonwebtoken use");

    // Applications commonly issue a token before constructing the decoder.
    let token = jsonwebtoken::encode(
        &Header::default(),
        &Claims {
            sub: "test".into(),
            exp: 4_000_000_000,
        },
        &EncodingKey::from_secret(b"secret"),
    )
    .expect("AWS-LC should be selected when both provider features are enabled");

    let _decoder = ConstDecoder::new(DecodingKey::from_secret(b"secret"));
    let decoded = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(b"secret"),
        &Validation::default(),
    )
    .expect("AWS-LC should verify tokens when both provider features are enabled");
    assert_eq!(decoded.claims.sub, "test");
}
