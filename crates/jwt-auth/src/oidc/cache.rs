// port from https://github.com/fergus-hou/oidc_jwt_validator/blob/master/src/cache.rs

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::Validation;
use salvo_core::http::header::HeaderValue;

use super::{current_time, decode_jwk, DecodingInfo, JwkSetFetch};

/// Determines settings about updating the cached JWKS data.
/// The JWKS will be lazily revalidated every time [validate](crate::Validator) validates a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachePolicy {
    /// Time in Seconds to refresh the JWKS from the OIDC Provider
    /// Default/Minimum value: 1 Second
    pub max_age: Duration,
    /// The amount of time a s
    pub stale_while_revalidate: Option<Duration>,
    /// The amount of time the stale JWKS data should be valid for if we are unable to re-validate it from the URL.
    /// Minimum Value: 60 Seconds
    pub stale_if_error: Option<Duration>,
}

impl CachePolicy {
    /// Create a new cache policy from the header value of the Cache-Control header
    pub fn from_header_val(value: Option<&HeaderValue>) -> Self {
        // Initalize the default config of polling every second
        let mut config = Self::default();

        if let Some(value) = value {
            if let Ok(value) = value.to_str() {
                config.parse_str(value);
            }
        }
        config
    }

    fn parse_str(&mut self, value: &str) {
        // Iterate over every token in the header value
        for token in value.split(',') {
            // split them into whitespace trimmed pairs
            let (key, val) = {
                let mut split = token.split('=').map(str::trim);
                (split.next(), split.next())
            };
            //Modify the default config based on the values that matter
            //Any values here would be more permisssive than the default behavior
            match (key, val) {
                (Some("max-age"), Some(val)) => {
                    if let Ok(secs) = val.parse::<u64>() {
                        self.max_age = Duration::from_secs(secs);
                    }
                }
                (Some("stale-while-revalidate"), Some(val)) => {
                    if let Ok(secs) = val.parse::<u64>() {
                        self.stale_while_revalidate = Some(Duration::from_secs(secs));
                    }
                }
                (Some("stale-if-error"), Some(val)) => {
                    if let Ok(secs) = val.parse::<u64>() {
                        self.stale_if_error = Some(Duration::from_secs(secs));
                    }
                }
                _ => continue,
            };
        }
    }
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            max_age: Duration::from_secs(1),
            stale_while_revalidate: Some(Duration::from_secs(1)),
            stale_if_error: Some(Duration::from_secs(60)),
        }
    }
}

/// The udpate action of cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAction {
    /// We checked the JWKS uri and it was the same as the last time we refreshed it so no action was taken
    NoUpdate,
    /// We checked the JWKS uri and it was different so we updated our local cache
    JwksUpdate,
    /// The JWKS Uri responded with a different cache-control header
    CacheUpdate(CachePolicy),
    /// The JWKS Uri responded with a different cache-control header and the JWKS was updated
    JwksAndCacheUpdate(CachePolicy),
}

/// Helper struct for determining when our cache needs to be re-validated
/// Utilizes atomics to prevent write-locking as much as possible
#[derive(Debug)]
pub struct CacheState {
    last_update: AtomicU64,
    is_revalidating: AtomicBool,
    is_error: AtomicBool,
}

impl CacheState {
    /// Create a new `CacheState`
    pub fn new() -> Self {
        Self {
            last_update: AtomicU64::new(current_time()),
            is_revalidating: AtomicBool::new(false),
            is_error: AtomicBool::new(false),
        }
    }
    /// Check is the cache is error
    pub fn is_error(&self) -> bool {
        self.is_error.load(Ordering::SeqCst)
    }
    /// Set the cache is error
    pub fn set_is_error(&self, value: bool) {
        self.is_error.store(value, Ordering::SeqCst);
    }

    /// Get the cache last updated timestamp
    pub fn last_update(&self) -> u64 {
        self.last_update.load(Ordering::SeqCst)
    }
    /// Set the cache last updated timestamp
    pub fn set_last_update(&self, timestamp: u64) {
        self.last_update.store(timestamp, Ordering::SeqCst);
    }

    /// Check if the cache is revalidating
    pub fn is_revalidating(&self) -> bool {
        self.is_revalidating.load(Ordering::SeqCst)
    }
    /// Set the cache is revalidating
    pub fn set_is_revalidating(&self, value: bool) {
        self.is_revalidating.store(value, Ordering::SeqCst);
    }
}

impl Default for CacheState {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper Struct for storing
pub struct JwkSetStore {
    /// The current JWKS
    pub jwks: JwkSet,
    decoding_map: HashMap<String, Arc<DecodingInfo>>,
    /// The cache policy for this store
    pub cache_policy: CachePolicy,
    validation: Validation,
}

impl JwkSetStore {
    /// Create a new `JwkSetStore`
    pub fn new(jwks: JwkSet, cache_policy: CachePolicy, validation: Validation) -> Self {
        Self {
            jwks,
            decoding_map: HashMap::new(),
            cache_policy,
            validation,
        }
    }

    fn update_jwks(&mut self, new_jwks: JwkSet) {
        self.jwks = new_jwks;
        let keys = self
            .jwks
            .keys
            .iter()
            .filter_map(|i| decode_jwk(i, &self.validation).ok());
        // Clear our cache of decoding keys
        self.decoding_map.clear();
        // Load the keys back into our hashmap cache.
        for key in keys {
            self.decoding_map.insert(key.0, Arc::new(key.1));
        }
    }

    /// Get the DecodingInfo for a given kid
    pub fn get_key(&self, kid: &str) -> Option<Arc<DecodingInfo>> {
        self.decoding_map.get(kid).cloned()
    }

    pub(crate) fn update_fetch(&mut self, fetch: JwkSetFetch) -> UpdateAction {
        tracing::debug!("Decoding JWKS");
        let time = Instant::now();
        let new_jwks = fetch.jwks;
        // If we didn't parse out a cache policy from the last request
        // Assume that it's the same as the last
        let cache_policy = fetch.cache_policy.unwrap_or(self.cache_policy);
        let result = match (self.jwks == new_jwks, self.cache_policy == cache_policy) {
            // Everything is the same
            (true, true) => {
                tracing::debug!("JWKS Content has not changed since last update");
                UpdateAction::NoUpdate
            }
            // The JWKS changed but the cache policy hasn't
            (false, true) => {
                tracing::info!("JWKS Content has changed since last update");
                self.update_jwks(new_jwks);
                UpdateAction::JwksUpdate
            }
            // The cache policy changed, but the JWKS hasn't
            (true, false) => {
                self.cache_policy = cache_policy;
                UpdateAction::CacheUpdate(cache_policy)
            }
            // Both the cache and the JWKS have changed
            (false, false) => {
                tracing::info!("cache-control header and JWKS content has changed since last update");
                self.update_jwks(new_jwks);
                self.cache_policy = cache_policy;
                UpdateAction::JwksAndCacheUpdate(cache_policy)
            }
        };
        let elapsed = time.elapsed();
        tracing::debug!("Decoded and parsed JWKS in {:#?}", elapsed);
        result
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn validate_headers() {
        let _input = vec![
            "max-age=604800",
            "no-cache",
            "max-age=604800, must-revalidate",
            "no-store",
            "public, max-age=604800, immutable",
            "max-age=604800, stale-while-revalidate=86400",
            "max-age=604800, stale-if-error=86400",
        ];
    }
}
