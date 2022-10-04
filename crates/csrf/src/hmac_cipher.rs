use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::CsrfCipher;

/// HmacCipher is a CSRF protection implementation that uses HMAC.
pub struct HmacCipher {
    hmac_key: [u8; 32],
    token_size: usize,
}

impl HmacCipher {
    /// Given an HMAC key, return an `HmacCipher` instance.
    #[inline]
    pub fn new(hmac_key: [u8; 32]) -> Self {
        Self { hmac_key, token_size: 32 }
    }

    /// Sets the length of the token.
    #[inline]
    pub fn with_token_size(mut self, token_size: usize) -> Self {
        assert!(token_size >= 8, "length must be larger than 8");
        self.token_size = token_size;
        self
    }

    #[inline]
    fn hmac(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(&self.hmac_key).expect("HMAC can take key of any size")
    }
}

impl CsrfCipher for HmacCipher {
    fn verify(&self, token: &[u8], secret: &[u8]) -> bool {
        if secret.len() != self.token_size {
            false
        } else {
            let token = token.to_vec();
            let mut hmac = self.hmac();
            hmac.update(&token);
            hmac.verify(secret.into()).is_ok()
        }
    }
    fn generate(&self) -> (Vec<u8>, Vec<u8>) {
        let token = self.random_bytes(self.token_size);
        let mut hmac = self.hmac();
        hmac.update(&token);
        let mac = hmac.finalize();
        let secret = mac.into_bytes();
        (token.to_vec(), secret.to_vec())
    }
}
