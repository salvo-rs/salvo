use std::io::{Error as IoError, Result as IoResult};

#[cfg(any(feature = "aws-lc-rs", not(feature = "ring")))]
use aws_lc_rs::{
    rand::SystemRandom,
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair as _, Signature},
};
#[cfg(all(not(feature = "aws-lc-rs"), feature = "ring"))]
use ring::{
    rand::SystemRandom,
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair as _, Signature},
};

pub(crate) struct KeyPair(EcdsaKeyPair);

impl KeyPair {
    #[inline]
    pub(crate) fn from_pkcs8(pkcs8: impl AsRef<[u8]>) -> IoResult<Self> {
        #[cfg(any(feature = "aws-lc-rs", not(feature = "ring")))]
        return EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref())
            .map(KeyPair)
            .map_err(|_| IoError::other("failed to load key pair"));
        #[cfg(all(not(feature = "aws-lc-rs"), feature = "ring"))]
        return EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_FIXED_SIGNING,
            pkcs8.as_ref(),
            &SystemRandom::new(),
        )
        .map(KeyPair)
        .map_err(|_| IoError::other("failed to load key pair"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_pair_generate() {
        let key_pair = KeyPair::generate();
        assert!(key_pair.is_ok());
    }

    #[test]
    fn test_key_pair_generate_unique() {
        let key_pair1 = KeyPair::generate().unwrap();
        let key_pair2 = KeyPair::generate().unwrap();

        // Each generated key pair should have a unique public key
        assert_ne!(key_pair1.public_key(), key_pair2.public_key());
    }

    #[test]
    fn test_key_pair_public_key_not_empty() {
        let key_pair = KeyPair::generate().unwrap();
        let public_key = key_pair.public_key();

        assert!(!public_key.is_empty());
        // P-256 public key should be 65 bytes (uncompressed format: 0x04 + 32 bytes x + 32 bytes y)
        assert_eq!(public_key.len(), 65);
        // First byte should be 0x04 for uncompressed point
        assert_eq!(public_key[0], 0x04);
    }

    #[test]
    fn test_key_pair_sign() {
        let key_pair = KeyPair::generate().unwrap();
        let message = b"test message to sign";

        let signature = key_pair.sign(message);
        assert!(signature.is_ok());

        let sig = signature.unwrap();
        let sig_bytes: &[u8] = sig.as_ref();
        // ECDSA P-256 signature should be 64 bytes (r: 32 bytes, s: 32 bytes)
        assert_eq!(sig_bytes.len(), 64);
    }

    #[test]
    fn test_key_pair_sign_different_messages() {
        let key_pair = KeyPair::generate().unwrap();
        let message1 = b"message one";
        let message2 = b"message two";

        let sig1 = key_pair.sign(message1).unwrap();
        let sig2 = key_pair.sign(message2).unwrap();

        // Different messages should produce different signatures
        let sig1_bytes: &[u8] = sig1.as_ref();
        let sig2_bytes: &[u8] = sig2.as_ref();
        assert_ne!(sig1_bytes, sig2_bytes);
    }

    #[test]
    fn test_key_pair_sign_empty_message() {
        let key_pair = KeyPair::generate().unwrap();
        let message = b"";

        let signature = key_pair.sign(message);
        assert!(signature.is_ok());
    }

    #[test]
    fn test_key_pair_sign_large_message() {
        let key_pair = KeyPair::generate().unwrap();
        let message = vec![0u8; 10000]; // 10KB message

        let signature = key_pair.sign(&message);
        assert!(signature.is_ok());
    }

    #[test]
    fn test_key_pair_from_pkcs8_invalid() {
        let invalid_pkcs8 = b"not a valid pkcs8 key";
        let result = KeyPair::from_pkcs8(invalid_pkcs8);
        assert!(result.is_err());
    }
}
