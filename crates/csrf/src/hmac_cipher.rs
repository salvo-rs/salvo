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
    pub fn token_size(mut self, token_size: usize) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let hmac_key = [0u8; 32];
        let hmac_cipher = HmacCipher::new(hmac_key);
        assert_eq!(hmac_cipher.hmac_key, hmac_key);
        assert_eq!(hmac_cipher.token_size, 32);
    }

    #[test]
    fn test_with_token_size() {
        let hmac_key = [0u8; 32];
        let hmac_cipher = HmacCipher::new(hmac_key).token_size(16);
        assert_eq!(hmac_cipher.token_size, 16);
    }

    #[test]
    fn test_verify() {
        let hmac_key = [0u8; 32];
        let hmac_cipher = HmacCipher::new(hmac_key);
        let (token, secret) = hmac_cipher.generate();
        assert!(hmac_cipher.verify(&token, &secret));
    }

    #[test]
    fn test_verify_invalid() {
        let hmac_key = [0u8; 32];
        let hmac_cipher = HmacCipher::new(hmac_key);
        let (token, _) = hmac_cipher.generate();
        let invalid_secret = vec![0u8; hmac_cipher.token_size];
        assert!(!hmac_cipher.verify(&token, &invalid_secret));
    }

    #[test]
    fn test_generate() {
        let hmac_key = [0u8; 32];
        let hmac_cipher = HmacCipher::new(hmac_key);
        let (token, secret) = hmac_cipher.generate();
        assert_eq!(token.len(), hmac_cipher.token_size);
        assert_eq!(secret.len(), hmac_cipher.token_size);
        assert!(hmac_cipher.verify(&token, &secret));
    }
}
