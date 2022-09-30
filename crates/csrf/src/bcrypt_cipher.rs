use super::CsrfCipher;

/// BcryptCipher is a CSRF protection implementation that uses bcrypt.
pub struct BcryptCipher {
    /// Cost for bcrypt.
    pub cost: u32,
    /// Length of the secret.
    pub len: usize,
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
        Self { cost: 8, len: 32 }
    }

    /// Set the length of the secret.
    #[inline]
    pub fn with_len(mut self, len: usize) -> Self {
        self.len = len;
        self
    }

    /// Set the cost for bcrypt.
    #[inline]
    pub fn with_cost(mut self, cost: u32) -> Self {
        self.cost = cost;
        self
    }
}

impl CsrfCipher for BcryptCipher {
    fn verify(&self, token: &[u8], secret: &[u8]) -> bool {
        bcrypt::verify(token, std::str::from_utf8(secret).unwrap_or_default()).unwrap_or(false)
    }
    fn generate(&self) -> (Vec<u8>, Vec<u8>) {
        let token = self.random_bytes(self.len);
        let secret = bcrypt::hash(&token, self.cost).unwrap();
        (token, secret.as_bytes().to_vec())
    }
}
