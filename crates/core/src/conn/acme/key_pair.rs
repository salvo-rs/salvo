use std::io::{Error as IoError, Result as IoResult};

use ring::{
    rand::SystemRandom,
    signature::{EcdsaKeyPair, KeyPair as _, Signature, ECDSA_P256_SHA256_FIXED_SIGNING},
};

pub(crate) struct KeyPair(EcdsaKeyPair);

impl KeyPair {
    #[inline]
    pub(crate) fn from_pkcs8(pkcs8: impl AsRef<[u8]>) -> IoResult<Self> {
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &SystemRandom::new())
            .map(KeyPair)
            .map_err(|_| IoError::other("failed to load key pair"))
    }

    #[inline]
    fn generate_pkcs8() -> IoResult<impl AsRef<[u8]>> {
        let alg = &ECDSA_P256_SHA256_FIXED_SIGNING;
        let rng = SystemRandom::new();
        EcdsaKeyPair::generate_pkcs8(alg, &rng)
            .map_err(|_| IoError::other("failed to generate acme key pair"))
    }

    #[inline]
    pub(crate) fn generate() -> IoResult<Self> {
        Self::from_pkcs8(Self::generate_pkcs8()?)
    }

    #[inline]
    pub(crate) fn sign(&self, message: impl AsRef<[u8]>) -> IoResult<Signature> {
        self.0
            .sign(&SystemRandom::new(), message.as_ref())
            .map_err(|_| IoError::other("failed to sign message"))
    }

    #[inline]
    pub(crate) fn public_key(&self) -> &[u8] {
        self.0.public_key().as_ref()
    }
}
