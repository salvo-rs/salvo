use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

use super::CsrfCipher;

/// CSRF protection implementation that uses bcrypt.
///
/// Middleware uses bounded blocking tasks, shared across all bcrypt ciphers in this process.
/// The synchronous [`CsrfCipher::generate`] and [`CsrfCipher::verify`] methods remain blocking.
/// Prefer a keyed HMAC/AEAD cipher for high-throughput applications.
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
    async fn verify_async(&self, token: &str, proof: &str) -> bool {
        if token.len() != (self.token_size * 4).div_ceil(3)
            || proof.len() != 60
            || !proof.is_ascii()
        {
            return false;
        }
        let cipher = self.clone();
        let token = token.to_owned();
        let proof = proof.to_owned();
        run_blocking(move || cipher.verify(&token, &proof))
            .await
            .unwrap_or(false)
    }

    async fn generate_async(&self) -> Result<(String, String), salvo_core::Error> {
        let cipher = self.clone();
        run_blocking(move || cipher.generate()).await
    }

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

// Acquire before spawning, and move the permit into the task. Cancellation of the request must
// not release capacity while already-started bcrypt work is still running.
async fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, salvo_core::Error> {
    static CAPACITY: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let capacity = CAPACITY.get_or_init(|| {
        Arc::new(Semaphore::new(
            std::thread::available_parallelism().map_or(1, usize::from),
        ))
    });
    run_with_capacity(capacity.clone(), work).await
}

async fn run_with_capacity<T: Send + 'static>(
    capacity: Arc<Semaphore>,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, salvo_core::Error> {
    let permit = capacity.acquire_owned().await.map_err(salvo_core::Error::other)?;
    struct Task<T>(tokio::task::JoinHandle<T>);
    impl<T> Drop for Task<T> {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let mut task = Task(tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    }));
    (&mut task.0).await.map_err(salvo_core::Error::other)
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    use super::*;

    #[tokio::test]
    async fn cancelled_request_keeps_capacity_until_blocking_work_finishes() {
        let capacity = Arc::new(Semaphore::new(1));
        let runtime_thread = std::thread::current().id();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first = tokio::spawn(run_with_capacity(capacity.clone(), move || {
            started_tx.send(std::thread::current().id()).unwrap();
            release_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        }));
        assert_ne!(started_rx.await.unwrap(), runtime_thread);
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        assert_eq!(capacity.available_permits(), 0);

        let (second_tx, mut second_rx) = tokio::sync::oneshot::channel();
        let second = tokio::spawn(run_with_capacity(capacity.clone(), move || {
            second_tx.send(()).unwrap();
        }));
        assert!(tokio::time::timeout(std::time::Duration::from_millis(30), &mut second_rx)
            .await.is_err());
        release_tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), second_rx)
            .await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(capacity.available_permits(), 1);
    }

    #[tokio::test]
    async fn failed_blocking_task_returns_an_error_and_releases_capacity() {
        let capacity = Arc::new(Semaphore::new(1));
        assert!(run_with_capacity(capacity.clone(), || panic!("test worker failure")).await.is_err());
        assert_eq!(capacity.available_permits(), 1);
    }

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
