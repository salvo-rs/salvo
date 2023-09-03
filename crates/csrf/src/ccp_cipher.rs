use aead::generic_array::GenericArray;
use aead::{Aead, KeyInit};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chacha20poly1305::ChaCha20Poly1305;

use super::CsrfCipher;

/// CcpCipher is a CSRF protection implementation that uses [`ChaCha20Poly1305`](https://datatracker.ietf.org/doc/html/rfc8439).
pub struct CcpCipher {
    aead_key: [u8; 32],
    token_size: usize,
}

impl CcpCipher {
    /// Given an aead key, return an `CcpCipher` instance.
    #[inline]
    pub fn new(aead_key: [u8; 32]) -> Self {
        Self {
            aead_key,
            token_size: 32,
        }
    }

    /// Sets the length of the token.
    #[inline]
    pub fn token_size(mut self, token_size: usize) -> Self {
        assert!(token_size >= 8, "length must be larger than 8");
        self.token_size = token_size;
        self
    }

    #[inline]
    fn aead(&self) -> ChaCha20Poly1305 {
        let key = GenericArray::clone_from_slice(&self.aead_key);
        ChaCha20Poly1305::new(&key)
    }
}

impl CsrfCipher for CcpCipher {
    fn verify(&self, token: &str, proof: &str) -> bool {
        if let (Ok(token), Ok(proof)) = (
            URL_SAFE_NO_PAD.decode(token.as_bytes()),
            URL_SAFE_NO_PAD.decode(proof.as_bytes()),
        ) {
            if token.len() < 8 || proof.len() < 20 {
                false
            } else {
                let nonce = GenericArray::from_slice(&proof[0..12]);
                let aead = self.aead();
                aead.decrypt(nonce, &proof[12..]).map(|p| p == token).unwrap_or(false)
            }
        } else {
            false
        }
    }
    fn generate(&self) -> (String, String) {
        let token = self.random_bytes(self.token_size);
        let aead = self.aead();
        let mut proof = self.random_bytes(12);
        let nonce = GenericArray::from_slice(&proof);
        proof.append(&mut aead.encrypt(nonce, token.as_slice()).unwrap());
        (URL_SAFE_NO_PAD.encode(token), URL_SAFE_NO_PAD.encode(proof))
    }
}
