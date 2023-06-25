use jsonwebtoken::errors::Error as JwtError;
use jsonwebtoken::{decode, Algorithm, DecodingKey, TokenData, Validation};
use serde::Deserialize;

use salvo_core::{async_trait, Depot};

/// JwtAuthDecoder is used to decode token to claims.
#[async_trait]
pub trait JwtAuthDecoder {
    /// Error type.
    type Error: std::error::Error;

    ///Decode token.
    async fn decode<C>(&self, token: &str, depot: &mut Depot) -> Result<TokenData<C>, Self::Error>
    where
        C: for<'de> Deserialize<'de>;
}

/// ConstDecoder will decode token with a static secret.
pub struct ConstDecoder {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl ConstDecoder {
    /// Create a new `ConstDecoder`.
    pub fn new(decoding_key: DecodingKey) -> Self {
        Self {
            decoding_key,
            validation: Validation::default(),
        }
    }
    /// Create a new `ConstDecoder` with validation.
    pub fn with_validation(decoding_key: DecodingKey, validation: Validation) -> Self {
        Self {
            decoding_key,
            validation,
        }
    }

    /// If you're using HMAC, use this.
    pub fn from_secret(secret: &[u8]) -> Self {
        Self::with_validation(DecodingKey::from_secret(secret), Validation::default())
    }

    /// If you're using HMAC with a base64 encoded secret, use this.
    pub fn from_base64_secret(secret: &str) -> Result<Self, JwtError> {
        DecodingKey::from_base64_secret(secret).map(|key| Self::with_validation(key, Validation::default()))
    }

    /// If you are loading a public RSA key in a PEM format, use this.
    /// Only exists if the feature `use_pem` is enabled.
    pub fn from_rsa_pem(key: &[u8]) -> Result<Self, JwtError> {
        DecodingKey::from_rsa_pem(key).map(|key| Self::with_validation(key, Validation::new(Algorithm::RS256)))
    }

    /// If you have (n, e) RSA public key components as strings, use this.
    pub fn from_rsa_components(modulus: &str, exponent: &str) -> Result<Self, JwtError> {
        DecodingKey::from_rsa_components(modulus, exponent)
            .map(|key| Self::with_validation(key, Validation::new(Algorithm::PS512)))
    }

    /// If you have (n, e) RSA public key components already decoded, use this.
    pub fn from_rsa_raw_components(modulus: &[u8], exponent: &[u8]) -> Self {
        Self::with_validation(
            DecodingKey::from_rsa_raw_components(modulus, exponent),
            Validation::new(Algorithm::PS512),
        )
    }

    /// If you have a ECDSA public key in PEM format, use this.
    /// Only exists if the feature `use_pem` is enabled.
    pub fn from_ec_pem(key: &[u8]) -> Result<Self, JwtError> {
        DecodingKey::from_ec_pem(key).map(|key| Self::with_validation(key, Validation::new(Algorithm::ES256)))
    }

    /// If you have (x,y) ECDSA key components
    pub fn from_ec_components(x: &str, y: &str) -> Result<Self, JwtError> {
        DecodingKey::from_ec_components(x, y).map(|key| Self::with_validation(key, Validation::new(Algorithm::ES256)))
    }

    /// If you have a EdDSA public key in PEM format, use this.
    /// Only exists if the feature `use_pem` is enabled.
    pub fn from_ed_pem(key: &[u8]) -> Result<Self, JwtError> {
        DecodingKey::from_ed_pem(key).map(|key| Self::with_validation(key, Validation::new(Algorithm::EdDSA)))
    }

    /// If you know what you're doing and have a RSA DER encoded public key, use this.
    pub fn from_rsa_der(der: &[u8]) -> Self {
        Self::with_validation(DecodingKey::from_rsa_der(der), Validation::new(Algorithm::RS256))
    }

    /// If you know what you're doing and have a RSA EC encoded public key, use this.
    pub fn from_ec_der(der: &[u8]) -> Self {
        Self::with_validation(DecodingKey::from_ec_der(der), Validation::new(Algorithm::ES256))
    }

    /// If you know what you're doing and have a Ed DER encoded public key, use this.
    pub fn from_ed_der(der: &[u8]) -> Self {
        Self::with_validation(DecodingKey::from_ed_der(der), Validation::new(Algorithm::EdDSA))
    }

    /// From x part (base64 encoded) of the JWK encoding
    pub fn from_ed_components(x: &str) -> Result<Self, JwtError> {
        DecodingKey::from_ed_components(x).map(|key| Self::with_validation(key, Validation::new(Algorithm::EdDSA)))
    }
}

#[async_trait]
impl JwtAuthDecoder for ConstDecoder {
    type Error = JwtError;

    async fn decode<C>(&self, token: &str, _depot: &mut Depot) -> Result<TokenData<C>, Self::Error>
    where
        C: for<'de> Deserialize<'de>,
    {
        decode::<C>(token, &self.decoding_key, &self.validation)
    }
}
