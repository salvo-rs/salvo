use jsonwebtoken::errors::Error as JwtError;
use jsonwebtoken::{decode, DecodingKey, TokenData, Validation};
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
    secret: String,
    validation: Validation,
}

impl ConstDecoder {
    /// Create a new `ConstDecoder`.
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
            validation: Validation::default(),
        }
    }
    /// Create a new `ConstDecoder` with validation.
    pub fn with_validation(secret: impl Into<String>, validation: Validation) -> Self {
        Self {
            secret: secret.into(),
            validation,
        }
    }
}

#[async_trait]
impl JwtAuthDecoder for ConstDecoder {
    type Error = JwtError;

    async fn decode<C>(&self, token: &str, _depot: &mut Depot) -> Result<TokenData<C>, Self::Error>
    where
        C: for<'de> Deserialize<'de>,
    {
        decode::<C>(token, &DecodingKey::from_secret(self.secret.as_ref()), &self.validation)
    }
}
