//! Crypto provider feature interaction tests.

#[cfg(all(feature = "aws-lc-rs", feature = "ring"))]
#[test]
fn both_provider_features_select_aws_lc() {
    use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
    use salvo_jwt_auth::{ConstDecoder, decode};
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Serialize)]
    struct Claims {
        sub: String,
        exp: u64,
    }

    // Constructing a Salvo decoder selects a deterministic process-wide
    // provider before applications use jsonwebtoken to issue tokens.
    let _decoder = ConstDecoder::new(DecodingKey::from_secret(b"secret"));
    let token = jsonwebtoken::encode(
        &Header::default(),
        &Claims {
            sub: "test".into(),
            exp: 4_000_000_000,
        },
        &EncodingKey::from_secret(b"secret"),
    )
    .expect("AWS-LC should be selected when both provider features are enabled");

    let decoded = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(b"secret"),
        &Validation::default(),
    )
    .expect("AWS-LC should verify tokens when both provider features are enabled");
    assert_eq!(decoded.claims.sub, "test");
}
