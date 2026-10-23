use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use super::CsrfCipher;

/// CSRF protection implementation that uses bcrypt.
#[derive(Debug, Clone)]
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            cost: 8,
            token_size: 32,
        }
    }

    /// Sets the length of the token.
    #[inline]
    #[must_use]
    pub fn token_size(mut self, token_size: usize) -> Self {
        assert!((8..=72).contains(&token_size), "length must be between 8 and 72");
        self.token_size = token_size;
        self
    }

    /// Sets the cost for bcrypt.
    ///
    /// Proofs are accepted only at this cost. Changing it invalidates existing
    /// tokens, which must be generated again with the new setting.
    #[inline]
    #[must_use]
    pub fn cost(mut self, cost: u32) -> Self {
        assert!((4..=31).contains(&cost), "cost must be between 4 and 31");
        self.cost = cost;
        self
    }
}

impl CsrfCipher for BcryptCipher {
    fn verify(&self, token: &str, proof: &str) -> bool {
        // Reject malformed input before allocating or entering bcrypt. The token and
        // hash format are public; only the hash comparison needs constant-time handling.
        if token.len() != (self.token_size * 4).div_ceil(3)
            || proof.len() != 60
            || !proof.is_ascii()
        {
            return false;
        }
        let Ok(token_bytes) = URL_SAFE_NO_PAD.decode(token.as_bytes()) else {
            return false;
        };
        if token_bytes.len() != self.token_size {
            return false;
        }

        // bcrypt hashes only use the `./0-9A-Za-z$` alphabet; `generate` rewrites
        // `/` to `_` for URL safety, so the inverse here is just `_` -> `/`.
        let proof = proof.replace('_', "/");
        let Ok(parts) = proof.parse::<bcrypt::HashParts>() else {
            return false;
        };
        // Verification uses the hash's embedded cost, not `self.cost`. Never let
        // an untrusted cookie choose more work than the configured generation cost.
        if parts.get_cost() != self.cost {
            return false;
        }

        bcrypt::verify(&token_bytes, &proof).unwrap_or(false)
    }
    fn generate(&self) -> (String, String) {
        let token = self.random_bytes(self.token_size);
        // bcrypt output never contains `+`; only `/` needs URL-safe rewriting.
        let proof = bcrypt::hash(&token, self.cost)
            .expect("bcrypt hash failed")
            .replace('/', "_");

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
    fn test_bcrypt_cipher_with_min_token_size() {
        let cipher = BcryptCipher::new().token_size(8);
        assert_eq!(cipher.token_size, 8);
    }

    #[test]
    #[should_panic(expected = "length must be between 8 and 72")]
    fn test_bcrypt_cipher_with_too_small_token_size() {
        let _ = BcryptCipher::new().token_size(7);
    }

    #[test]
    #[should_panic(expected = "length must be between 8 and 72")]
    fn test_bcrypt_cipher_with_too_large_token_size() {
        let _ = BcryptCipher::new().token_size(73);
    }

    #[test]
    fn test_bcrypt_cipher_with_cost() {
        let cipher = BcryptCipher::new().cost(10);
        assert_eq!(cipher.cost, 10);
    }

    #[test]
    #[should_panic(expected = "cost must be between 4 and 31")]
    fn test_bcrypt_cipher_with_invalid_cost() {
        let _ = BcryptCipher::new().cost(32);
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

    #[test]
    fn test_bcrypt_cipher_rejects_a_different_cost() {
        // Keep both test costs small: rejection is tested with a real hash, not
        // by ever evaluating an expensive attacker-selected cost.
        let cipher = BcryptCipher::new().cost(4);
        let (token, proof) = BcryptCipher::new().cost(5).generate();
        assert!(!cipher.verify(&token, &proof));
    }

    #[test]
    fn test_bcrypt_cipher_rejects_malformed_input() {
        let cipher = BcryptCipher::new().cost(4);
        let (token, proof) = cipher.generate();
        assert!(!cipher.verify(&format!("{token}A"), &proof));
        assert!(!cipher.verify(&token, &format!("{proof}A")));
        assert!(!cipher.verify(&"!".repeat(token.len()), &proof));
        // A non-ASCII string with the expected byte length must not reach the
        // dependency's ASCII hash parser.
        let non_ascii = format!("{}é", &proof[..58]);
        assert!(!cipher.verify(&token, &non_ascii));
    }
}
