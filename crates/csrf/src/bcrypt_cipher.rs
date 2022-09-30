use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::Cursor;

use hmac::{Hmac, Mac};

use super::CsrfCipher;

pub struct BcryptCipher {
    cost: usize,
    len: usize,
}

impl BcryptCipher {
    /// Given an HMAC key, return an `HmacCsrfProtection` instance.
    #[inline]
    pub fn new() -> Self {
        Self { cost: 8, len: 32 }
    }
    #[inline]
    pub fn with_len(mut self, len: usize) -> Self {
        self.len = len;
        self
    }
    #[inline]
    pub fn with_cost(mut self, cost: usize) -> Self {
        self.cost = cost;
        self
    }
}

impl CsrfCipher for BcryptCipher {
    fn verify(&self, secret: &[u8], token: &[u8]) -> bool {
        bcrypt::verify(secret, token).unwrap_or(false)
    }
    fn generate(&self) -> (Vec<u8>, Vec<u8>) {
        let mut secret = self.random_bytes(self.len);
        let token = bcrypt::hash(&secret, self.cost).unwrap();
        (secret.to_vec(), token.to_vec())
    }
}
