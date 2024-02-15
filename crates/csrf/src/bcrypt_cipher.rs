use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use super::CsrfCipher;

/// CSRF protection implementation that uses bcrypt.
pub struct BcryptCipher {
    cost: u32,
    token_size: usize,
}
impl Default for BcryptCipher {
    fn default() -> Self {
        Self::new()
    }
}

impl BcryptCipher {
    /// Create a new `BcryptCipher`.
    #[inline]
    pub fn new() -> Self {
        Self {
            cost: 8,
            token_size: 32,
        }
    }

    /// Sets the length of the token.
    #[inline]
    pub fn token_size(mut self, token_size: usize) -> Self {
        assert!((1..=72).contains(&token_size), "length must be between 1 and 72");
        self.token_size = token_size;
        self
    }

    /// Sets the cost for bcrypt.
    #[inline]
    pub fn cost(mut self, cost: u32) -> Self {
        assert!((4..=31).contains(&cost), "cost must be between 4 and 31");
        self.cost = cost;
        self
    }
}

impl CsrfCipher for BcryptCipher {
    fn verify(&self, token: &str, proof: &str) -> bool {
        if let Ok(token) = URL_SAFE_NO_PAD.decode(token.as_bytes()) {
            let proof = proof.replace('_', "/").replace('-', "+");
            bcrypt::verify(token, &proof).unwrap_or(false)
        } else {
            false
        }
    }
    fn generate(&self) -> (String, String) {
        let token = self.random_bytes(self.token_size);
        let proof = bcrypt::hash(&token, self.cost).expect("Call bcrypt hash get error result.").replace('+', "/").replace('/', "_");

        (URL_SAFE_NO_PAD.encode(token), proof)
    }
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    use super::*;

    #[test]
    fn test_bcrypt_cipher_new() {
        let cipher = BcryptCipher::new();
        assert_eq!(cipher.cost, 8);
        assert_eq!(cipher.token_size, 32);
    }

    #[test]
    fn test_bcrypt_cipher_with_token_size() {
        let cipher = BcryptCipher::new().token_size(16);
        assert_eq!(cipher.token_size, 16);
    }

    #[test]
    #[should_panic(expected = "length must be between 1 and 72")]
    fn test_bcrypt_cipher_with_invalid_token_size() {
        BcryptCipher::new().token_size(0);
    }

    #[test]
    fn test_bcrypt_cipher_with_cost() {
        let cipher = BcryptCipher::new().cost(10);
        assert_eq!(cipher.cost, 10);
    }

    #[test]
    #[should_panic(expected = "cost must be between 4 and 31")]
    fn test_bcrypt_cipher_with_invalid_cost() {
        BcryptCipher::new().cost(32);
    }

    #[test]
    fn test_bcrypt_cipher_verify_and_generate() {
        let cipher = BcryptCipher::new();
        let (token, proof) = cipher.generate();
        assert!(cipher.verify(&token, &proof));
    }

    #[test]
    fn test_bcrypt_cipher_verify_invalid_token() {
        let cipher = BcryptCipher::new();
        let (token, proof) = cipher.generate();
        let invalid_token = URL_SAFE_NO_PAD.encode(vec![0; token.len()]);
        assert!(!cipher.verify(&invalid_token, &proof));
    }
}
