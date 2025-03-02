use jsonwebtoken::errors::Error as JwtError;
use jsonwebtoken::{Algorithm, DecodingKey, TokenData, Validation, decode};
use serde::Deserialize;

use salvo_core::Depot;

/// Trait for JWT token decoding and validation.
///
/// Implementors of this trait are responsible for decoding JWT tokens into claims objects
/// and performing any necessary validation. The `JwtAuth` middleware uses the configured
/// decoder to validate tokens extracted from requests.
///
/// The crate provides built-in implementations:
/// - `ConstDecoder`: Uses a static key for token validation
/// - `OidcDecoder`: Uses OpenID Connect for validation (requires the `oidc` feature)
pub trait JwtAuthDecoder {
    /// The error type returned if decoding or validation fails.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Decodes and validates a JWT token.
    ///
    /// # Parameters
    ///
    /// * `token` - The JWT token string to decode
    /// * `depot` - The current request's depot, which can be used to store/retrieve additional data
    ///
    /// # Type Parameters
    ///
    /// * `C` - The claims type to deserialize from the token payload
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing either the decoded token data or an error.
    fn decode<C>(
        &self,
        token: &str,
        depot: &mut Depot,
    ) -> impl Future<Output = Result<TokenData<C>, Self::Error>> + Send
    where
        C: for<'de> Deserialize<'de>;
}

/// A decoder that uses a constant key for JWT token validation.
///
/// This is the simplest decoder implementation, suitable for applications using
/// symmetric key signing (HMAC) or a single asymmetric key pair (RSA, ECDSA).
pub struct ConstDecoder {
    /// Key used for validating JWT signatures
    decoding_key: DecodingKey,

    /// JWT validation parameters
    validation: Validation,
}

impl ConstDecoder {
    /// Creates a new decoder with the given decoding key and default validation.
    pub fn new(decoding_key: DecodingKey) -> Self {
        Self {
            decoding_key,
            validation: Validation::default(),
        }
    }

    /// Creates a new decoder with the given decoding key and custom validation parameters.
    pub fn with_validation(decoding_key: DecodingKey, validation: Validation) -> Self {
        Self {
            decoding_key,
            validation,
        }
    }

    /// Creates a decoder from a raw secret byte array for HMAC verification.
    ///
    /// This is the most common method for symmetric key validation.
    pub fn from_secret(secret: &[u8]) -> Self {
        Self::with_validation(DecodingKey::from_secret(secret), Validation::default())
    }

    /// Creates a decoder from a base64-encoded secret string for HMAC verification.
    pub fn from_base64_secret(secret: &str) -> Result<Self, JwtError> {
        DecodingKey::from_base64_secret(secret)
            .map(|key| Self::with_validation(key, Validation::default()))
    }

    /// Creates a decoder from an RSA public key in PEM format.
    ///
    /// Only available when the `use_pem` feature is enabled.
    pub fn from_rsa_pem(key: &[u8]) -> Result<Self, JwtError> {
        DecodingKey::from_rsa_pem(key)
            .map(|key| Self::with_validation(key, Validation::new(Algorithm::RS256)))
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
        DecodingKey::from_ec_pem(key)
            .map(|key| Self::with_validation(key, Validation::new(Algorithm::ES256)))
    }

    /// If you have (x,y) ECDSA key components
    pub fn from_ec_components(x: &str, y: &str) -> Result<Self, JwtError> {
        DecodingKey::from_ec_components(x, y)
            .map(|key| Self::with_validation(key, Validation::new(Algorithm::ES256)))
    }

    /// If you have a EdDSA public key in PEM format, use this.
    /// Only exists if the feature `use_pem` is enabled.
    pub fn from_ed_pem(key: &[u8]) -> Result<Self, JwtError> {
        DecodingKey::from_ed_pem(key)
            .map(|key| Self::with_validation(key, Validation::new(Algorithm::EdDSA)))
    }

    /// If you know what you're doing and have a RSA DER encoded public key, use this.
    pub fn from_rsa_der(der: &[u8]) -> Self {
        Self::with_validation(
            DecodingKey::from_rsa_der(der),
            Validation::new(Algorithm::RS256),
        )
    }

    /// If you know what you're doing and have a RSA EC encoded public key, use this.
    pub fn from_ec_der(der: &[u8]) -> Self {
        Self::with_validation(
            DecodingKey::from_ec_der(der),
            Validation::new(Algorithm::ES256),
        )
    }

    /// If you know what you're doing and have a Ed DER encoded public key, use this.
    pub fn from_ed_der(der: &[u8]) -> Self {
        Self::with_validation(
            DecodingKey::from_ed_der(der),
            Validation::new(Algorithm::EdDSA),
        )
    }

    /// From x part (base64 encoded) of the JWK encoding
    pub fn from_ed_components(x: &str) -> Result<Self, JwtError> {
        DecodingKey::from_ed_components(x)
            .map(|key| Self::with_validation(key, Validation::new(Algorithm::EdDSA)))
    }
}

impl JwtAuthDecoder for ConstDecoder {
    type Error = JwtError;

    async fn decode<C>(&self, token: &str, _depot: &mut Depot) -> Result<TokenData<C>, Self::Error>
    where
        C: for<'de> Deserialize<'de>,
    {
        decode::<C>(token, &self.decoding_key, &self.validation)
    }
}
