//! Oidc(OpenID Connect) supports.

use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::TokioExecutor;
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, DecodingKey, TokenData, Validation};
use salvo_core::http::{header::CACHE_CONTROL, uri::Uri};
use salvo_core::Depot;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tokio::sync::{Notify, RwLock};

use super::{JwtAuthDecoder, JwtAuthError};

mod cache;

pub use cache::{CachePolicy, CacheState, JwkSetStore, UpdateAction};

pub(super) type HyperClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>;

/// ConstDecoder will decode token with a static secret.
#[derive(Clone)]
pub struct OidcDecoder {
    issuer: String,
    http_client: HyperClient,
    cache: Arc<RwLock<JwkSetStore>>,
    cache_state: Arc<CacheState>,
    notifier: Arc<Notify>,
}

impl JwtAuthDecoder for OidcDecoder {
    type Error = JwtAuthError;

    /// Validates a JWT, Returning the claims serialized into type of T
    async fn decode<C>(&self, token: &str, _depot: &mut Depot) -> Result<TokenData<C>, Self::Error>
    where
        C: DeserializeOwned,
    {
        // Early return error conditions before acquiring a read lock
        let header = jsonwebtoken::decode_header(token)?;
        let kid = header.kid.ok_or(JwtAuthError::MissingKid)?;

        let decoding_key = self.get_kid_retry(kid).await?;
        let decoded = decoding_key.decode(token)?;
        Ok(decoded)
    }
}

/// A builder for `OidcDecoder`.
pub struct DecoderBuilder<T>
where
    T: AsRef<str>,
{
    /// The issuer URL of the token. eg: `https://xx-xx.clerk.accounts.dev`
    pub issuer: T,
    /// The http client for the decoder.
    pub http_client: Option<HyperClient>,
    /// The validation options for the decoder.
    pub validation: Option<Validation>,
}
impl<T> DecoderBuilder<T>
where
    T: AsRef<str>,
{
    /// Create a new `DecoderBuilder`.
    pub fn new(issuer: T) -> Self {
        Self {
            issuer,
            http_client: None,
            validation: None,
        }
    }
    /// Set the http client for the decoder.
    pub fn http_client(mut self, client: HyperClient) -> Self {
        self.http_client = Some(client);
        self
    }
    /// Set the validation options for the decoder.
    pub fn validation(mut self, validation: Validation) -> Self {
        self.validation = Some(validation);
        self
    }

    /// Build a `OidcDecoder`.
    pub fn build(self) -> impl Future<Output = Result<OidcDecoder, JwtAuthError>> {
        let Self {
            issuer,
            http_client,
            validation,
        } = self;
        let issuer = issuer.as_ref().trim_end_matches('/').to_string();

        //Create an empty JWKS to initalize our Cache
        let jwks = JwkSet { keys: Vec::new() };

        let validation = validation.unwrap_or_default();
        let cache = Arc::new(RwLock::new(JwkSetStore::new(jwks, CachePolicy::default(), validation)));
        let cache_state = Arc::new(CacheState::new());

        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .expect("no native root CA certificates found")
            .https_only()
            .enable_http1()
            .build();
        let http_client = http_client.unwrap_or_else(|| Client::builder(TokioExecutor::new()).build(https));
        let decoder = OidcDecoder {
            issuer,
            http_client,
            cache,
            cache_state,
            notifier: Arc::new(Notify::new()),
        };
        async move {
            decoder.update_cache().await?;
            Ok(decoder)
        }
    }
}

impl OidcDecoder {
    /// Create a new `OidcDecoder`.
    pub fn new(issuer: impl AsRef<str>) -> impl Future<Output = Result<Self, JwtAuthError>> {
        Self::builder(issuer).build()
    }

    /// Create a new `DecoderBuilder`.
    pub fn builder<T>(issuer: T) -> DecoderBuilder<T>
    where
        T: AsRef<str>,
    {
        DecoderBuilder::new(issuer)
    }

    fn config_url(&self) -> String {
        format!("{}/.well-known/openid-configuration", &self.issuer)
    }
    async fn get_config(&self) -> Result<OidcConfig, JwtAuthError> {
        let res = self.http_client.get(self.config_url().parse::<Uri>()?).await?;
        let body = res.into_body().collect().await?.to_bytes();
        let config = serde_json::from_slice(&body)?;
        Ok(config)
    }
    async fn jwks_uri(&self) -> Result<String, JwtAuthError> {
        Ok(self.get_config().await?.jwks_uri)
    }

    /// Triggers an HTTP Request to get a fresh `JwkSet`
    async fn get_jwks(&self) -> Result<JwkSetFetch, JwtAuthError> {
        let uri = self.jwks_uri().await?.parse::<Uri>()?;
        // Get the jwks endpoint
        tracing::debug!("Requesting JWKS From Uri: {uri}");
        let res = self.http_client.get(uri).await?;

        let cache_policy = {
            // Determine it from the cache_control header
            let cache_control = res.headers().get(CACHE_CONTROL);
            let cache_policy = CachePolicy::from_header_val(cache_control);
            Some(cache_policy)
        };
        let jwks = res.into_body().collect().await?.to_bytes();

        let fetched_at = current_time();
        Ok(JwkSetFetch {
            jwks: serde_json::from_slice(&jwks)?,
            cache_policy,
            fetched_at,
        })
    }

    /// Triggers an immediate update from the JWKS URL
    /// Will only write lock the [`JwkSetStore`] if there is an actual change to the contents.
    async fn update_cache(&self) -> Result<UpdateAction, JwtAuthError> {
        let fetch = self.get_jwks().await;
        match fetch {
            Ok(fetch) => {
                self.cache_state.set_last_update(fetch.fetched_at);
                tracing::info!("Set Last update to {:#?}", fetch.fetched_at);
                self.cache_state.set_is_error(false);
                let read = self.cache.read().await;

                if read.jwks == fetch.jwks && fetch.cache_policy.unwrap_or(read.cache_policy) == read.cache_policy {
                    return Ok(UpdateAction::NoUpdate);
                }
                drop(read);
                let mut write = self.cache.write().await;

                Ok(write.update_fetch(fetch))
            }
            Err(e) => {
                self.cache_state.set_is_error(true);
                Err(e)
            }
        }
    }
    /// Triggers an eventual update from the JWKS URL
    /// Will only ever spawn one task at a single time.
    /// If called while an update task is currently running, will do nothing.
    fn revalidate_cache(&self) {
        if !self.cache_state.is_revalidating() {
            self.cache_state.set_is_revalidating(true);
            tracing::info!("Spawning Task to re-validate JWKS");
            let a = self.clone();
            tokio::task::spawn(async move {
                let _ = a.update_cache().await;
                a.cache_state.set_is_revalidating(false);
                a.notifier.notify_waiters();
            });
        }
    }

    /// If we are currently updating the JWKS in the background this function will resolve when the update it complete
    /// If we are not currently updating the JWKS in the backgroun, this function will resolve immediatly.
    async fn wait_update(&self) {
        if self.cache_state.is_revalidating() {
            self.notifier.notified().await;
        }
    }

    /// Primary method for getting the [`DecodingInfo`] for a JWK needed to validate a JWT.
    /// If the kid was not present in [`JwkSetStore`]
    #[allow(clippy::future_not_send)]
    async fn get_kid_retry(&self, kid: impl AsRef<str>) -> Result<Arc<DecodingInfo>, JwtAuthError> {
        let kid = kid.as_ref();
        // Check to see if we have the kid
        if let Ok(Some(key)) = self.get_kid(kid).await {
            // if we have it, then return it
            Ok(key)
        } else {
            // Try and invalidate our cache. Maybe the JWKS has changed or our cached values expired
            // Even if it failed it. It may allow us to retrieve a key from stale-if-error
            self.revalidate_cache();
            self.wait_update().await;
            self.get_kid(kid).await?.ok_or(JwtAuthError::CacheError)
        }
    }

    /// Gets the decoding components of a JWK by kid from the JWKS in our cache
    /// Returns an Error, if the cache is stale and beyond the Stale While Revalidate and Stale If Error allowances configured in [`crate::cache::Settings`]
    /// Returns Ok if the cache is not stale.
    /// Returns Ok after triggering a background update of the JWKS If the cache is stale but within the Stale While Revalidate and Stale If Error allowances.
    #[allow(clippy::future_not_send)]
    async fn get_kid(&self, kid: &str) -> Result<Option<Arc<DecodingInfo>>, JwtAuthError> {
        let read_cache = self.cache.read().await;
        let fetched = self.cache_state.last_update();
        let max_age_secs = read_cache.cache_policy.max_age.as_secs();

        let max_age = fetched + max_age_secs;
        let now = current_time();
        let val = read_cache.get_key(kid);

        if now <= max_age {
            return Ok(val);
        }

        // If the stale while revalidate setting is present
        if let Some(swr) = read_cache.cache_policy.stale_while_revalidate {
            // if we're within the SWR allowed window
            if now <= swr.as_secs() + max_age {
                self.revalidate_cache();
                return Ok(val);
            }
        }
        if let Some(swr_err) = read_cache.cache_policy.stale_if_error {
            // if the last update failed and the stale-if-error is present
            if now <= swr_err.as_secs() + max_age && self.cache_state.is_error() {
                self.revalidate_cache();
                return Ok(val);
            }
        }
        drop(read_cache);
        tracing::info!("Returning None: {now} - {max_age}");
        Err(JwtAuthError::CacheError)
    }
}

/// Struct used to store the computed information needed to decode a JWT
/// Intended to be cached inside of [`JwkSetStore`] to prevent decoding information about the same JWK more than once
pub struct DecodingInfo {
    // jwk: Jwk,
    key: DecodingKey,
    validation: Validation,
    // alg: Algorithm,
}
impl DecodingInfo {
    fn new(key: DecodingKey, alg: Algorithm, validation_settings: &Validation) -> Self {
        let mut validation = Validation::new(alg);

        validation.aud.clone_from(&validation_settings.aud);
        validation.iss.clone_from(&validation_settings.iss);
        validation.leeway = validation_settings.leeway;
        validation.required_spec_claims.clone_from(&validation_settings.required_spec_claims);

        validation.sub.clone_from(&validation_settings.sub);
        validation.validate_exp = validation_settings.validate_exp;
        validation.validate_nbf = validation_settings.validate_nbf;

        Self {
            // jwk,
            key,
            validation,
            // alg,
        }
    }

    fn decode<T>(&self, token: &str) -> Result<TokenData<T>, JwtAuthError>
    where
        T: for<'de> serde::de::Deserialize<'de>,
    {
        match jsonwebtoken::decode::<T>(token, &self.key, &self.validation) {
            Ok(data) => Ok(data),
            Err(e) => {
                tracing::error!(error = ?e, token, "error decoding jwt token");
                Err(JwtAuthError::from(e))
            }
        }
    }
}

/// Helper Stuct that contains the response of a request to the jwks uri
/// `cache_policy` will be Some when [`cache::Strategy`] is set to [`cache::Strategy::Automatic`].
#[derive(Debug)]
pub(crate) struct JwkSetFetch {
    jwks: JwkSet,
    cache_policy: Option<CachePolicy>,
    fetched_at: u64,
}

#[derive(Debug, Deserialize)]
struct OidcConfig {
    jwks_uri: String,
}

pub(crate) fn decode_jwk(jwk: &Jwk, validation: &Validation) -> Result<(String, DecodingInfo), JwtAuthError> {
    let kid = jwk.common.key_id.clone();
    let alg = jwk.common.key_algorithm;

    let dec_key = match jwk.algorithm {
        jsonwebtoken::jwk::AlgorithmParameters::EllipticCurve(ref params) => {
            let x_cmp = b64_decode(&params.x)?;
            let y_cmp = b64_decode(&params.y)?;
            let mut public_key = Vec::with_capacity(1 + params.x.len() + params.y.len());
            public_key.push(0x04);
            public_key.extend_from_slice(&x_cmp);
            public_key.extend_from_slice(&y_cmp);
            Some(DecodingKey::from_ec_der(&public_key))
        }
        jsonwebtoken::jwk::AlgorithmParameters::RSA(ref params) => {
            DecodingKey::from_rsa_components(&params.n, &params.e).ok()
        }
        jsonwebtoken::jwk::AlgorithmParameters::OctetKey(ref params) => {
            DecodingKey::from_base64_secret(&params.value).ok()
        }
        jsonwebtoken::jwk::AlgorithmParameters::OctetKeyPair(ref params) => {
            let der = b64_decode(&params.x)?;

            Some(DecodingKey::from_ed_der(&der))
        }
    };
    match (kid, alg, dec_key) {
        (Some(kid), Some(alg), Some(dec_key)) => {
            let alg = Algorithm::from_str(alg.to_string().as_str())?;
            let info = DecodingInfo::new(dec_key, alg, validation);
            Ok((kid, info))
        }
        _ => Err(JwtAuthError::InvalidJwk),
    }
}

fn b64_decode<T: AsRef<[u8]>>(input: T) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD.decode(input.as_ref())
}

pub(crate) fn current_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time Went Backwards")
        .as_secs()
}
