use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::CsrfCipher;

/// A CSRF protection implementation that uses HMAC.
pub struct HmacCipher {
    hmac_key: [u8; 32],
    token_size: usize,
}

impl HmacCipher {
    /// Given an HMAC key, return an `HmacCipher` instance.
    #[inline]
    pub fn new(hmac_key: [u8; 32]) -> Self {
        Self {
            hmac_key,
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
    fn hmac(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(&self.hmac_key).expect("HMAC can take key of any size")
    }
}

impl CsrfCipher for HmacCipher {
    fn verify(&self, token: &str, proof: &str) -> bool {
        if let (Ok(token), Ok(proof)) = (
            URL_SAFE_NO_PAD.decode(token.as_bytes()),
            URL_SAFE_NO_PAD.decode(proof.as_bytes()),
        ) {
            if proof.len() != self.token_size {
                false
            } else {
                let mut hmac = self.hmac();
                hmac.update(&token);
                hmac.verify((&*proof).into()).is_ok()
            }
        } else {
            false
        }
    }
    fn generate(&self) -> (String, String) {
        let token = self.random_bytes(self.token_size);
        let mut hmac = self.hmac();
        hmac.update(&token);
        let mac = hmac.finalize();
        let proof = mac.into_bytes();
        (URL_SAFE_NO_PAD.encode(token), URL_SAFE_NO_PAD.encode(proof))
    }
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

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
        let (token, proof) = hmac_cipher.generate();
        assert!(hmac_cipher.verify(&token, &proof));
    }

    #[test]
    fn test_verify_invalid() {
        let hmac_key = [0u8; 32];
        let hmac_cipher = HmacCipher::new(hmac_key);
        let (token, _) = hmac_cipher.generate();
        let invalid_proof = URL_SAFE_NO_PAD.encode(vec![0u8; hmac_cipher.token_size]);
        assert!(!hmac_cipher.verify(&token, &invalid_proof));
    }

    #[test]
    fn test_generate() {
        let hmac_key = [0u8; 32];
        let hmac_cipher = HmacCipher::new(hmac_key);
        let (token, proof) = hmac_cipher.generate();
        assert!(hmac_cipher.verify(&token, &proof));
    }
}
