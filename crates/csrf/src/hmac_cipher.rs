use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::CsrfCipher;

/// HmacCipher is a CSRF protection implementation that uses HMAC.
pub struct HmacCipher {
    key: [u8; 32],
    len: usize,
}

impl HmacCipher {
    /// Given an HMAC key, return an `HmacCipher` instance.
    #[inline]
    pub fn new(key: [u8; 32]) -> Self {
        Self { key, len: 32 }
    }

    /// Set the length of the secret.
    #[inline]
    pub fn with_len(mut self, len: usize) -> Self {
        self.len = len;
        self
    }

    #[inline]
    fn hmac(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(&self.key).expect("HMAC can take key of any size")
    }
}

impl CsrfCipher for HmacCipher {
    fn verify(&self, token: &[u8], secret: &[u8]) -> bool {
        if secret.len() != self.len {
            false
        } else {
            let token = token.to_vec();
            let mut hmac = self.hmac();
            hmac.update(&token);
            hmac.verify(secret.into()).is_ok()
        }
    }
    fn generate(&self) -> (Vec<u8>, Vec<u8>) {
        let token = self.random_bytes(self.len);
        let mut hmac = self.hmac();
        hmac.update(&token);
        let mac = hmac.finalize();
        let secret = mac.into_bytes();
        (token.to_vec(), secret.to_vec())
    }
}
