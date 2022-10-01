use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::CsrfCipher;

/// HmacCipher is a CSRF protection implementation that uses HMAC.
pub struct HmacCipher {
    hmac_key: Vec<u8>,
    token_len: usize,
}

impl HmacCipher {
    /// Given an HMAC key, return an `HmacCipher` instance.
    #[inline]
    pub fn new(hmac_key: impl Into<Vec<u8>>) -> Self {
        Self { hmac_key: hmac_key.into(), token_len: 32 }
    }

    /// Set the length of the token.
    #[inline]
    pub fn with_token_len(mut self, token_len: usize) -> Self {
        assert!(token_len >= 8, "length must be larger than 8");
        self.token_len = token_len;
        self
    }

    #[inline]
    fn hmac(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(&self.hmac_key).expect("HMAC can take key of any size")
    }
}

impl CsrfCipher for HmacCipher {
    fn verify(&self, token: &[u8], secret: &[u8]) -> bool {
        if secret.len() != self.token_len {
            false
        } else {
            let token = token.to_vec();
            let mut hmac = self.hmac();
            hmac.update(&token);
            hmac.verify(secret.into()).is_ok()
        }
    }
    fn generate(&self) -> (Vec<u8>, Vec<u8>) {
        let token = self.random_bytes(self.token_len);
        let mut hmac = self.hmac();
        hmac.update(&token);
        let mac = hmac.finalize();
        let secret = mac.into_bytes();
        (token.to_vec(), secret.to_vec())
    }
}
