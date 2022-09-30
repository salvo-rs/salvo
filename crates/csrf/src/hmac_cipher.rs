
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::CsrfCipher;

pub struct HmacCipher {
    key: [u8; 32],
    len: usize,
}

impl HmacCipher {
    /// Given an HMAC key, return an `HmacCsrfProtection` instance.
    #[inline]
    pub fn new(key: [u8; 32]) -> Self {
        Self { key, len: 32 }
    }
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
    fn verify(&self, secret: &[u8], token: &[u8]) -> bool {
        if token.len() != self.len {
            false
        } else {
            let mut secret = secret.to_vec();
            let mut hmac = self.hmac();
            hmac.update(&mut secret);
            hmac.verify(token.into()).is_ok()
        }
    }
    fn generate(&self) -> (Vec<u8>, Vec<u8>) {
        let mut secret = self.random_bytes(self.len);
        let mut hmac = self.hmac();
        hmac.update(&mut secret);
        let mac = hmac.finalize();
        let token = mac.into_bytes();
        (secret.to_vec(), token.to_vec())
    }
}
