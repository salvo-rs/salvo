//! Crypto provider feature interaction tests.

#[cfg(all(feature = "aws-lc-rs", feature = "ring"))]
#[tokio::test]
async fn explicit_initialization_precedes_direct_and_salvo_jwt_use() {
    use jsonwebtoken::{DecodingKey, EncodingKey, Header};
    use salvo_core::Depot;
    use salvo_jwt_auth::{ConstDecoder, JwtAuthDecoder, install_crypto_provider};
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Serialize)]
    struct Claims {
        sub: String,
        exp: u64,
    }

    install_crypto_provider().expect("the provider must be installed before any JWT use");

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

    let decoder = ConstDecoder::new(DecodingKey::from_secret(b"secret"));
    let decoded = decoder
        .decode::<Claims>(&token, &mut Depot::new())
        .await
        .expect("AWS-LC should verify tokens when both provider features are enabled");
    assert_eq!(decoded.claims.sub, "test");
}
