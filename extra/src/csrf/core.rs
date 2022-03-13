//! Module containing the core functionality for CSRF protection
//! port from https://raw.githubusercontent.com/heartsucker/rust-csrf/master/src/core.rs

use std::error::Error;
use std::io::Cursor;
use std::fmt;

use aead::{generic_array::GenericArray, Aead, NewAead};
use aes_gcm::Aes256Gcm;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use chacha20poly1305::ChaCha20Poly1305;
use chrono::prelude::*;
use chrono::Duration;
use data_encoding::{BASE64, BASE64URL};
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;

/// An `enum` of all CSRF related errors.
#[derive(Debug, Hash, Eq, PartialEq, Clone)]
pub enum CsrfError {
    /// There was an internal error.
    InternalError,
    /// There was CSRF token validation failure.
    ValidationFailure,
    /// There was a CSRF token encryption failure.
    EncryptionFailure,
}

impl Error for CsrfError {}

impl fmt::Display for CsrfError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CsrfError::InternalError => write!(f, "CSRF library error"),
            CsrfError::ValidationFailure => write!(f, "CSRF validation failed"),
            CsrfError::EncryptionFailure => write!(f, "CSRF encryption failed"),
        }
    }
}

/// A signed, encrypted CSRF token that is suitable to be displayed to end users.
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub struct CsrfToken {
    bytes: Vec<u8>,
}

impl CsrfToken {
    /// Create a new token from the given bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        // TODO make this return a Result and check that bytes is long enough
        CsrfToken { bytes: bytes }
    }

    /// Retrieve the CSRF token as a base64 encoded string.
    pub fn b64_string(&self) -> String {
        BASE64.encode(&self.bytes)
    }

    /// Retrieve the CSRF token as a URL safe base64 encoded string.
    pub fn b64_url_string(&self) -> String {
        BASE64URL.encode(&self.bytes)
    }

    /// Get be raw value of this token.
    pub fn value(&self) -> &[u8] {
        &self.bytes
    }
}

/// A signed, encrypted CSRF cookie that is suitable to be displayed to end users.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct CsrfCookie {
    bytes: Vec<u8>,
}

impl CsrfCookie {
    /// Create a new cookie from the given token bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        // TODO make this return a Result and check that bytes is long enough
        CsrfCookie { bytes: bytes }
    }

    /// Get the base64 value of this cookie.
    pub fn b64_string(&self) -> String {
        BASE64.encode(&self.bytes)
    }

    /// Get be raw value of this cookie.
    pub fn value(&self) -> &[u8] {
        &self.bytes
    }
}

/// Internal represenation of an unencrypted CSRF token. This is not suitable to send to end users.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct UnencryptedCsrfToken {
    token: Vec<u8>,
}

impl UnencryptedCsrfToken {
    /// Create a new unenrypted token.
    pub fn new(token: Vec<u8>) -> Self {
        UnencryptedCsrfToken { token: token }
    }

    /// Retrieve the token value as bytes.
    #[deprecated]
    pub fn token(&self) -> &[u8] {
        &self.token
    }

    /// Retrieve the token value as bytes.
    pub fn value(&self) -> &[u8] {
        &self.token
    }
}

/// Internal represenation of an unencrypted CSRF cookie. This is not suitable to send to end users.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct UnencryptedCsrfCookie {
    expires: i64,
    token: Vec<u8>,
}

impl UnencryptedCsrfCookie {
    /// Create a new unenrypted cookie.
    pub fn new(expires: i64, token: Vec<u8>) -> Self {
        UnencryptedCsrfCookie {
            expires: expires,
            token: token,
        }
    }

    /// Retrieve the token value as bytes.
    pub fn value(&self) -> &[u8] {
        &self.token
    }
}

/// The base trait that allows a developer to add CSRF protection to an application.
pub trait CsrfProtection: Send + Sync {
    /// Given a nonce and a time to live (TTL), create a cookie to send to the end user.
    fn generate_cookie(
        &self,
        token_value: &[u8; 64],
        ttl_seconds: i64,
    ) -> Result<CsrfCookie, CsrfError>;

    /// Given a nonce, create a token to send to the end user.
    fn generate_token(&self, token_value: &[u8; 64]) -> Result<CsrfToken, CsrfError>;

    /// Given a decoded byte array, deserialize, decrypt, and verify the cookie.
    fn parse_cookie(&self, cookie: &[u8]) -> Result<UnencryptedCsrfCookie, CsrfError>;

    /// Given a decoded byte array, deserialize, decrypt, and verify the token.
    fn parse_token(&self, token: &[u8]) -> Result<UnencryptedCsrfToken, CsrfError>;

    /// Given a token pair that has been parsed, decoded, decrypted, and verified, return whether
    /// or not the token matches the cookie and they have not expired.
    fn verify_token_pair(
        &self,
        token: &UnencryptedCsrfToken,
        cookie: &UnencryptedCsrfCookie,
    ) -> bool {
        let tokens_match = token.token == cookie.token;
        if !tokens_match {
            tracing::debug!(
                "Token did not match cookie: T: {:?}, C: {:?}",
                BASE64.encode(&token.token),
                BASE64.encode(&cookie.token)
            );
        }

        let now = Utc::now().timestamp();
        let not_expired = cookie.expires > now;
        if !not_expired {
            tracing::debug!(
                "Cookie expired. Expiration: {}, Current time: {}",
                cookie.expires, now
            );
        }

        tokens_match && not_expired
    }

    /// Given a buffer, fill it with random bytes or error if this is not possible.
    fn random_bytes(&self, buf: &mut [u8]) -> Result<(), CsrfError> {
        // TODO We had to get rid of `ring` because of `gcc` conflicts with `rust-crypto`, and
        // `ring`'s RNG didn't require mutability. Now create a new one per call which is not a
        // great idea.
        OsRng.fill_bytes(buf);
        Ok(())
    }

    /// Given an optional previous token and a TTL, generate a matching token and cookie pair.
    fn generate_token_pair(
        &self,
        previous_token_value: Option<&[u8; 64]>,
        ttl_seconds: i64,
    ) -> Result<(CsrfToken, CsrfCookie), CsrfError> {
        let token = match previous_token_value {
            Some(ref previous) => *previous.clone(),
            None => {
                tracing::debug!("Generating new CSRF token.");
                let mut token = [0; 64];
                self.random_bytes(&mut token)?;
                token
            }
        };

        match (
            self.generate_token(&token),
            self.generate_cookie(&token, ttl_seconds),
        ) {
            (Ok(t), Ok(c)) => Ok((t, c)),
            _ => Err(CsrfError::ValidationFailure),
        }
    }
}

/// Uses HMAC to provide authenticated CSRF tokens and cookies.
pub struct HmacCsrfProtection {
    hmac_key: [u8; 32],
}

impl HmacCsrfProtection {
    /// Given an HMAC key, return an `HmacCsrfProtection` instance.
    pub fn from_key(hmac_key: [u8; 32]) -> Self {
        HmacCsrfProtection { hmac_key: hmac_key }
    }

    fn hmac(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(&self.hmac_key).expect("HMAC can take key of any size")
    }
}

impl CsrfProtection for HmacCsrfProtection {
    fn generate_cookie(
        &self,
        token_value: &[u8; 64],
        ttl_seconds: i64,
    ) -> Result<CsrfCookie, CsrfError> {
        let expires = (Utc::now() + Duration::seconds(ttl_seconds)).timestamp();
        let mut expires_bytes = [0u8; 8];
        (&mut expires_bytes[..])
            .write_i64::<BigEndian>(expires)
            .map_err(|_| CsrfError::InternalError)?;

        let mut hmac = self.hmac();
        hmac.update(&expires_bytes);
        hmac.update(token_value);
        let mac = hmac.finalize();
        let code = mac.into_bytes();

        let mut transport = [0; 104];
        transport[0..32].copy_from_slice(&code);
        transport[32..40].copy_from_slice(&expires_bytes);
        transport[40..].copy_from_slice(token_value);

        Ok(CsrfCookie::new(transport.to_vec()))
    }

    fn generate_token(&self, token_value: &[u8; 64]) -> Result<CsrfToken, CsrfError> {
        let mut hmac = self.hmac();
        hmac.update(token_value);
        let mac = hmac.finalize();
        let code = mac.into_bytes();

        let mut transport = [0; 96];
        transport[0..32].copy_from_slice(&code);
        transport[32..].copy_from_slice(token_value);

        Ok(CsrfToken::new(transport.to_vec()))
    }

    fn parse_cookie(&self, cookie: &[u8]) -> Result<UnencryptedCsrfCookie, CsrfError> {
        if cookie.len() != 104 {
            tracing::debug!("Cookie wrong size. Not parsed.");
            return Err(CsrfError::ValidationFailure);
        }

        let mut hmac = self.hmac();
        hmac.update(&cookie[32..]);

        if hmac.verify(cookie[0..32].into()).is_err() {
            tracing::info!("CSRF cookie had bad MAC");
            return Err(CsrfError::ValidationFailure);
        }

        let mut cur = Cursor::new(&cookie[32..40]);
        let expires = cur
            .read_i64::<BigEndian>()
            .map_err(|_| CsrfError::InternalError)?;
        Ok(UnencryptedCsrfCookie::new(expires, cookie[40..].to_vec()))
    }

    fn parse_token(&self, token: &[u8]) -> Result<UnencryptedCsrfToken, CsrfError> {
        if token.len() != 96 {
            tracing::debug!("Token too small. Not parsed.");
            return Err(CsrfError::ValidationFailure);
        }

        let mut hmac = self.hmac();
        hmac.update(&token[32..]);

        if hmac.verify(token[0..32].into()).is_err() {
            tracing::info!("CSRF token had bad MAC");
            return Err(CsrfError::ValidationFailure);
        }

        Ok(UnencryptedCsrfToken::new(token[32..].to_vec()))
    }
}

/// Uses AES-GCM to provide signed, encrypted CSRF tokens and cookies.
pub struct AesGcmCsrfProtection {
    aead_key: [u8; 32],
}

impl AesGcmCsrfProtection {
    /// Given an AES256 key, return an `AesGcmCsrfProtection` instance.
    pub fn from_key(aead_key: [u8; 32]) -> Self {
        AesGcmCsrfProtection { aead_key: aead_key }
    }

    fn aead(&self) -> Aes256Gcm {
        let key = GenericArray::clone_from_slice(&self.aead_key);
        Aes256Gcm::new(&key)
    }
}

impl CsrfProtection for AesGcmCsrfProtection {
    fn generate_cookie(
        &self,
        token_value: &[u8; 64],
        ttl_seconds: i64,
    ) -> Result<CsrfCookie, CsrfError> {
        let expires = (Utc::now() + Duration::seconds(ttl_seconds)).timestamp();
        let mut expires_bytes = [0u8; 8];
        (&mut expires_bytes[..])
            .write_i64::<BigEndian>(expires)
            .map_err(|_| CsrfError::InternalError)?;

        let mut nonce = [0; 12];
        self.random_bytes(&mut nonce)?;

        let mut plaintext = [0; 104];
        self.random_bytes(&mut plaintext[0..32])?; // padding
        plaintext[32..40].copy_from_slice(&expires_bytes);
        plaintext[40..].copy_from_slice(token_value);

        let aead = self.aead();

        let nonce = GenericArray::from_slice(&nonce);
        let ciphertext = aead
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|_| CsrfError::EncryptionFailure)?;

        let mut transport = [0; 132];
        transport[0..12].copy_from_slice(&nonce);
        transport[12..].copy_from_slice(&ciphertext);

        Ok(CsrfCookie::new(transport.to_vec()))
    }

    fn generate_token(&self, token_value: &[u8; 64]) -> Result<CsrfToken, CsrfError> {
        let mut nonce = [0; 12];
        self.random_bytes(&mut nonce)?;

        let mut plaintext = [0; 96];
        self.random_bytes(&mut plaintext[0..32])?; // padding
        plaintext[32..].copy_from_slice(token_value);

        let aead = self.aead();

        let nonce = GenericArray::from_slice(&nonce);
        let ciphertext = aead
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|_| CsrfError::EncryptionFailure)?;

        let mut transport = [0; 124];
        transport[0..12].copy_from_slice(&nonce);
        transport[12..].copy_from_slice(&ciphertext);

        Ok(CsrfToken::new(transport.to_vec()))
    }

    fn parse_cookie(&self, cookie: &[u8]) -> Result<UnencryptedCsrfCookie, CsrfError> {
        if cookie.len() != 132 {
            tracing::debug!("Cookie wrong size. Not parsed.");
            return Err(CsrfError::ValidationFailure);
        }

        let mut nonce = [0; 12];
        nonce.copy_from_slice(&cookie[0..12]);

        let aead = self.aead();

        let nonce = GenericArray::from_slice(&nonce);
        let plaintext = aead.decrypt(nonce, cookie[12..].as_ref()).map_err(|_| {
            tracing::info!("Failed to decrypt CSRF cookie");
            CsrfError::ValidationFailure
        })?;

        let mut cur = Cursor::new(&plaintext[32..40]);
        let expires = cur
            .read_i64::<BigEndian>()
            .map_err(|_| CsrfError::InternalError)?;
        Ok(UnencryptedCsrfCookie::new(
            expires,
            plaintext[40..].to_vec(),
        ))
    }

    fn parse_token(&self, token: &[u8]) -> Result<UnencryptedCsrfToken, CsrfError> {
        if token.len() != 124 {
            tracing::debug!("Token too small. Not parsed.");
            return Err(CsrfError::ValidationFailure);
        }

        let mut nonce = [0; 12];
        nonce.copy_from_slice(&token[0..12]);

        let aead = self.aead();

        let nonce = GenericArray::from_slice(&nonce);
        let plaintext = aead.decrypt(nonce, token[12..].as_ref()).map_err(|_| {
            tracing::info!("Failed to decrypt CSRF token");
            CsrfError::ValidationFailure
        })?;

        Ok(UnencryptedCsrfToken::new(plaintext[32..].to_vec()))
    }
}

/// Uses ChaCha20Poly1305 to provide signed, encrypted CSRF tokens and cookies.
pub struct ChaCha20Poly1305CsrfProtection {
    aead_key: [u8; 32],
}

impl ChaCha20Poly1305CsrfProtection {
    /// Given a key, return a `ChaCha20Poly1305CsrfProtection` instance.
    pub fn from_key(aead_key: [u8; 32]) -> Self {
        ChaCha20Poly1305CsrfProtection { aead_key: aead_key }
    }

    fn aead(&self) -> ChaCha20Poly1305 {
        let key = GenericArray::clone_from_slice(&self.aead_key);
        ChaCha20Poly1305::new(&key)
    }
}

impl CsrfProtection for ChaCha20Poly1305CsrfProtection {
    fn generate_cookie(
        &self,
        token_value: &[u8; 64],
        ttl_seconds: i64,
    ) -> Result<CsrfCookie, CsrfError> {
        let expires = (Utc::now() + Duration::seconds(ttl_seconds)).timestamp();
        let mut expires_bytes = [0u8; 8];
        (&mut expires_bytes[..])
            .write_i64::<BigEndian>(expires)
            .map_err(|_| CsrfError::InternalError)?;

        let mut nonce = [0; 12];
        self.random_bytes(&mut nonce)?;

        let mut plaintext = [0; 104];
        self.random_bytes(&mut plaintext[0..32])?; // padding
        plaintext[32..40].copy_from_slice(&expires_bytes);
        plaintext[40..].copy_from_slice(token_value);

        let aead = self.aead();

        let nonce = GenericArray::from_slice(&nonce);
        let ciphertext = aead
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|_| CsrfError::EncryptionFailure)?;

        let mut transport = [0; 132];
        transport[0..12].copy_from_slice(&nonce);
        transport[12..].copy_from_slice(&ciphertext);

        Ok(CsrfCookie::new(transport.to_vec()))
    }

    fn generate_token(&self, token_value: &[u8; 64]) -> Result<CsrfToken, CsrfError> {
        let mut nonce = [0; 12];
        self.random_bytes(&mut nonce)?;

        let mut plaintext = [0; 96];
        self.random_bytes(&mut plaintext[0..32])?; // padding
        plaintext[32..].copy_from_slice(token_value);

        let aead = self.aead();

        let nonce = GenericArray::from_slice(&nonce);
        let ciphertext = aead
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|_| CsrfError::EncryptionFailure)?;

        let mut transport = [0; 124];
        transport[0..12].copy_from_slice(&nonce);
        transport[12..].copy_from_slice(&ciphertext);

        Ok(CsrfToken::new(transport.to_vec()))
    }

    fn parse_cookie(&self, cookie: &[u8]) -> Result<UnencryptedCsrfCookie, CsrfError> {
        if cookie.len() != 132 {
            tracing::debug!("Cookie wrong size. Not parsed.");
            return Err(CsrfError::ValidationFailure);
        }

        let mut nonce = [0; 12];
        nonce.copy_from_slice(&cookie[0..12]);

        let aead = self.aead();

        let nonce = GenericArray::from_slice(&nonce);
        let plaintext = aead.decrypt(nonce, cookie[12..].as_ref()).map_err(|_| {
            tracing::info!("Failed to decrypt CSRF cookie");
            CsrfError::ValidationFailure
        })?;

        let mut cur = Cursor::new(&plaintext[32..40]);
        let expires = cur
            .read_i64::<BigEndian>()
            .map_err(|_| CsrfError::InternalError)?;
        Ok(UnencryptedCsrfCookie::new(
            expires,
            plaintext[40..].to_vec(),
        ))
    }

    fn parse_token(&self, token: &[u8]) -> Result<UnencryptedCsrfToken, CsrfError> {
        if token.len() != 124 {
            tracing::debug!("Token too small. Not parsed.");
            return Err(CsrfError::ValidationFailure);
        }

        let mut nonce = [0; 12];
        nonce.copy_from_slice(&token[0..12]);

        let aead = self.aead();

        let nonce = GenericArray::from_slice(&nonce);
        let plaintext = aead.decrypt(nonce, token[12..].as_ref()).map_err(|_| {
            tracing::info!("Failed to decrypt CSRF token");
            CsrfError::ValidationFailure
        })?;

        Ok(UnencryptedCsrfToken::new(plaintext[32..].to_vec()))
    }
}

/// This is used when one wants to rotate keys or switch from implementation to another. It accepts
/// `1 + N` instances of `CsrfProtection` and uses only the first to generate tokens and cookies.
/// The `N` remaining instances are used only for parsing.
pub struct MultiCsrfProtection {
    current: Box<dyn CsrfProtection>,
    previous: Vec<Box<dyn CsrfProtection>>,
}

impl MultiCsrfProtection {
    /// Create a new `MultiCsrfProtection` from one current `CsrfProtection` and some `N` previous
    /// instances of `CsrfProtection`.
    pub fn new(current: Box<dyn CsrfProtection>, previous: Vec<Box<dyn CsrfProtection>>) -> Self {
        Self { current, previous }
    }
}

impl CsrfProtection for MultiCsrfProtection {
    fn generate_cookie(
        &self,
        token_value: &[u8; 64],
        ttl_seconds: i64,
    ) -> Result<CsrfCookie, CsrfError> {
        self.current.generate_cookie(token_value, ttl_seconds)
    }

    fn generate_token(&self, token_value: &[u8; 64]) -> Result<CsrfToken, CsrfError> {
        self.current.generate_token(token_value)
    }

    fn parse_cookie(&self, cookie: &[u8]) -> Result<UnencryptedCsrfCookie, CsrfError> {
        match self.current.parse_cookie(cookie) {
            ok @ Ok(_) => return ok,
            Err(_) => {
                for protection in self.previous.iter() {
                    match protection.parse_cookie(cookie) {
                        ok @ Ok(_) => return ok,
                        Err(_) => (),
                    }
                }
            }
        }
        Err(CsrfError::ValidationFailure)
    }

    fn parse_token(&self, token: &[u8]) -> Result<UnencryptedCsrfToken, CsrfError> {
        match self.current.parse_token(token) {
            ok @ Ok(_) => return ok,
            Err(_) => {
                for protection in self.previous.iter() {
                    match protection.parse_token(token) {
                        ok @ Ok(_) => return ok,
                        Err(_) => (),
                    }
                }
            }
        }
        Err(CsrfError::ValidationFailure)
    }
}


#[cfg(test)]
mod tests {
    // TODO write test that ensures encrypted messages don't contain the plaintext
    // TODO test that checks tokens are repeated when given Some

    const KEY_32: [u8; 32] = *b"01234567012345670123456701234567";
    const KEY2_32: [u8; 32] = *b"76543210765432107654321076543210";

    macro_rules! test_cases {
        ($strct: ident, $md: ident) => {
            mod $md {
                use super::KEY_32;
                use data_encoding::BASE64;
                use $crate::csrf::core::{$strct, CsrfProtection};

                #[test]
                fn verification_succeeds() {
                    let protect = $strct::from_key(KEY_32);
                    let (token, cookie) = protect
                        .generate_token_pair(None, 300)
                        .expect("couldn't generate token/cookie pair");
                    let ref token = BASE64
                        .decode(token.b64_string().as_bytes())
                        .expect("token not base64");
                    let token = protect.parse_token(&token).expect("token not parsed");
                    let ref cookie = BASE64
                        .decode(cookie.b64_string().as_bytes())
                        .expect("cookie not base64");
                    let cookie = protect.parse_cookie(&cookie).expect("cookie not parsed");
                    assert!(
                        protect.verify_token_pair(&token, &cookie),
                        "could not verify token/cookie pair"
                    );
                }

                #[test]
                fn modified_cookie_value_fails() {
                    let protect = $strct::from_key(KEY_32);
                    let (_, mut cookie) = protect
                        .generate_token_pair(None, 300)
                        .expect("couldn't generate token/cookie pair");
                    cookie.bytes[0] ^= 0x01;
                    let ref cookie = BASE64
                        .decode(cookie.b64_string().as_bytes())
                        .expect("cookie not base64");
                    assert!(protect.parse_cookie(&cookie).is_err());
                }

                #[test]
                fn modified_token_value_fails() {
                    let protect = $strct::from_key(KEY_32);
                    let (mut token, _) = protect
                        .generate_token_pair(None, 300)
                        .expect("couldn't generate token/token pair");
                    token.bytes[0] ^= 0x01;
                    let ref token = BASE64
                        .decode(token.b64_string().as_bytes())
                        .expect("token not base64");
                    assert!(protect.parse_token(&token).is_err());
                }

                #[test]
                fn mismatched_cookie_token_fail() {
                    let protect = $strct::from_key(KEY_32);
                    let (token, _) = protect
                        .generate_token_pair(None, 300)
                        .expect("couldn't generate token/token pair");
                    let (_, cookie) = protect
                        .generate_token_pair(None, 300)
                        .expect("couldn't generate token/token pair");

                    let ref token = BASE64
                        .decode(token.b64_string().as_bytes())
                        .expect("token not base64");
                    let token = protect.parse_token(&token).expect("token not parsed");
                    let ref cookie = BASE64
                        .decode(cookie.b64_string().as_bytes())
                        .expect("cookie not base64");
                    let cookie = protect.parse_cookie(&cookie).expect("cookie not parsed");
                    assert!(
                        !protect.verify_token_pair(&token, &cookie),
                        "verified token/cookie pair when failure expected"
                    );
                }

                #[test]
                fn expired_token_fail() {
                    let protect = $strct::from_key(KEY_32);
                    let (token, cookie) = protect
                        .generate_token_pair(None, -1)
                        .expect("couldn't generate token/cookie pair");
                    let ref token = BASE64
                        .decode(token.b64_string().as_bytes())
                        .expect("token not base64");
                    let token = protect.parse_token(&token).expect("token not parsed");
                    let ref cookie = BASE64
                        .decode(cookie.b64_string().as_bytes())
                        .expect("cookie not base64");
                    let cookie = protect.parse_cookie(&cookie).expect("cookie not parsed");
                    assert!(
                        !protect.verify_token_pair(&token, &cookie),
                        "verified token/cookie pair when failure expected"
                    );
                }
            }
        };
    }

    test_cases!(AesGcmCsrfProtection, aesgcm);
    test_cases!(ChaCha20Poly1305CsrfProtection, chacha20poly1305);
    test_cases!(HmacCsrfProtection, hmac);

    mod multi {
        macro_rules! test_cases {
            ($strct1: ident, $strct2: ident, $name: ident) => {
                mod $name {
                    use super::super::super::*;
                    use super::super::{KEY2_32, KEY_32};
                    use data_encoding::BASE64;

                    #[test]
                    fn no_previous() {
                        let protect = $strct1::from_key(KEY_32);
                        let mut pairs = vec![];
                        let pair = protect
                            .generate_token_pair(None, 300)
                            .expect("couldn't generate token/cookie pair");
                        pairs.push(pair);

                        let protect = MultiCsrfProtection::new(Box::new(protect), vec![]);
                        let pair = protect
                            .generate_token_pair(None, 300)
                            .expect("couldn't generate token/cookie pair");
                        pairs.push(pair);

                        for &(ref token, ref cookie) in pairs.iter() {
                            let ref token = BASE64
                                .decode(token.b64_string().as_bytes())
                                .expect("token not base64");
                            let token = protect.parse_token(&token).expect("token not parsed");
                            let ref cookie = BASE64
                                .decode(cookie.b64_string().as_bytes())
                                .expect("cookie not base64");
                            let cookie = protect.parse_cookie(&cookie).expect("cookie not parsed");
                            assert!(
                                protect.verify_token_pair(&token, &cookie),
                                "could not verify token/cookie pair"
                            );
                        }
                    }

                    #[test]
                    fn $name() {
                        let protect_1 = $strct1::from_key(KEY_32);
                        let mut pairs = vec![];
                        let pair = protect_1
                            .generate_token_pair(None, 300)
                            .expect("couldn't generate token/cookie pair");
                        pairs.push(pair);

                        let protect_2 = $strct2::from_key(KEY2_32);
                        let mut pairs = vec![];
                        let pair = protect_2
                            .generate_token_pair(None, 300)
                            .expect("couldn't generate token/cookie pair");
                        pairs.push(pair);

                        let protect = MultiCsrfProtection::new(
                            Box::new(protect_1),
                            vec![Box::new(protect_2)],
                        );
                        let pair = protect
                            .generate_token_pair(None, 300)
                            .expect("couldn't generate token/cookie pair");
                        pairs.push(pair);

                        for &(ref token, ref cookie) in pairs.iter() {
                            let ref token = BASE64
                                .decode(token.b64_string().as_bytes())
                                .expect("token not base64");
                            let token = protect.parse_token(&token).expect("token not parsed");
                            let ref cookie = BASE64
                                .decode(cookie.b64_string().as_bytes())
                                .expect("cookie not base64");
                            let cookie = protect.parse_cookie(&cookie).expect("cookie not parsed");
                            assert!(
                                protect.verify_token_pair(&token, &cookie),
                                "could not verify token/cookie pair"
                            );
                        }
                    }
                }
            };
        }

        test_cases!(
            AesGcmCsrfProtection,
            AesGcmCsrfProtection,
            aesgcm_then_aesgcm
        );

        test_cases!(
            ChaCha20Poly1305CsrfProtection,
            ChaCha20Poly1305CsrfProtection,
            chacha20poly1305_then_chacha20poly1305
        );

        test_cases!(HmacCsrfProtection, HmacCsrfProtection, hmac_then_hmac);

        test_cases!(
            ChaCha20Poly1305CsrfProtection,
            AesGcmCsrfProtection,
            chacha20poly1305_then_aesgcm
        );

        test_cases!(HmacCsrfProtection, AesGcmCsrfProtection, hmac_then_aesgcm);

        test_cases!(
            AesGcmCsrfProtection,
            ChaCha20Poly1305CsrfProtection,
            aesgcm_then_chacha20poly1305
        );
        test_cases!(
            HmacCsrfProtection,
            ChaCha20Poly1305CsrfProtection,
            hmac_then_chacha20poly1305
        );

        test_cases!(AesGcmCsrfProtection, HmacCsrfProtection, aesgcm_then_hmac);
        test_cases!(
            ChaCha20Poly1305CsrfProtection,
            HmacCsrfProtection,
            chacha20poly1305_then_hmac
        );
    }
}
