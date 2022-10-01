use super::CsrfCipher;

/// BcryptCipher is a CSRF protection implementation that uses bcrypt.
pub struct BcryptCipher {
    cost: u32,
    token_len: usize,
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
        Self { cost: 8, token_len: 32 }
    }

    /// Set the length of the token.
    #[inline]
    pub fn with_token_len(mut self, token_len: usize) -> Self {
        assert!(token_len >= 1 && token_len <= 72, "length must be between 1 and 72");
        self.token_len = token_len;
        self
    }

    /// Set the cost for bcrypt.
    #[inline]
    pub fn with_cost(mut self, cost: u32) -> Self {
        assert!(cost >=4 && cost <= 31, "cost must be between 4 and 31");
        self.cost = cost;
        self
    }
}

impl CsrfCipher for BcryptCipher {
    fn verify(&self, token: &[u8], secret: &[u8]) -> bool {
        bcrypt::verify(token, std::str::from_utf8(secret).unwrap_or_default()).unwrap_or(false)
    }
    fn generate(&self) -> (Vec<u8>, Vec<u8>) {
        let token = self.random_bytes(self.token_len);
        let secret = bcrypt::hash(&token, self.cost).unwrap();
        (token, secret.as_bytes().to_vec())
    }
}
