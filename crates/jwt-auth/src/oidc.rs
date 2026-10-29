//! OIDC (OpenID Connect) support.

use std::fmt::{self, Debug, Formatter};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;
use jsonwebtoken::jwk::{Jwk, JwkSet, KeyOperations, PublicKeyUse};
use jsonwebtoken::{Algorithm, DecodingKey, TokenData, Validation};
use salvo_core::Depot;
use salvo_core::http::{HeaderValue, header::{AGE, CACHE_CONTROL}, uri::Uri};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::sync::{Notify, RwLock};
use tokio::time::timeout;

use super::{JwtAuthDecoder, JwtAuthError};

mod cache;

pub use cache::{CachePolicy, CacheState, JwkSetStore, UpdateAction};

pub(super) type HyperClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>;

const OIDC_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const OIDC_CONFIG_MAX_BYTES: usize = 64 * 1024;
const OIDC_JWKS_MAX_BYTES: usize = 1024 * 1024;
// Match the minimum cache lifetime. A cache miss may trigger an early refresh,
// but sequential unknown kids must not bypass the provider's cache policy.
const OIDC_REVALIDATION_INTERVAL: Duration = Duration::from_secs(1);

fn default_http_client() -> Result<HyperClient, JwtAuthError> {
    let https = HttpsConnectorBuilder::new()
        .with_provider_and_native_roots(default_rustls_crypto_provider())
        .map_err(|error| JwtAuthError::NativeRootCerts(error.to_string()))?
        .https_only()
        .enable_http1()
        // Also negotiate HTTP/2: some IdP discovery/JWKS endpoints (often behind a
        // CDN) prefer or require h2 via ALPN, and http1-only would fail the handshake.
        .enable_http2()
        .build();
    Ok(Client::builder(TokioExecutor::new()).build(https))
}

fn default_rustls_crypto_provider() -> Arc<rustls::crypto::CryptoProvider> {
    if let Some(provider) = rustls::crypto::CryptoProvider::get_default() {
        return Arc::clone(provider);
    }

    #[cfg(feature = "aws-lc-rs")]
    {
        Arc::new(rustls::crypto::aws_lc_rs::default_provider())
    }
    #[cfg(all(not(feature = "aws-lc-rs"), feature = "ring"))]
    {
        Arc::new(rustls::crypto::ring::default_provider())
    }
    #[cfg(not(any(feature = "aws-lc-rs", feature = "ring")))]
    unreachable!("a crypto provider feature is required")
}

fn resolve_or_else<T, E>(
    provided: Option<T>,
    build_default: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    match provided {
        Some(value) => Ok(value),
        None => build_default(),
    }
}

/// Decodes JWTs using OpenID Connect discovery and JWKS.
#[derive(Clone)]
pub struct OidcDecoder {
    issuer: String,
    http_client: HyperClient,
    cache: Arc<RwLock<JwkSetStore>>,
    cache_state: Arc<CacheState>,
    notifier: Arc<Notify>,
}

impl Debug for OidcDecoder {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("OidcDecoder")
            .field("issuer", &self.issuer)
            .field("http_client", &self.http_client)
            .field("cache_state", &self.cache_state)
            .finish()
    }
}

impl JwtAuthDecoder for OidcDecoder {
    type Error = JwtAuthError;

    /// Validates a JWT, returning the claims deserialized into type `T`.
    async fn decode<C>(&self, token: &str, _depot: &mut Depot) -> Result<TokenData<C>, Self::Error>
    where
        C: DeserializeOwned + Clone,
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
    /// The expected OIDC token audiences.
    pub audiences: Option<Vec<String>>,
    /// Whether to accept symmetric (`kty: "oct"` / HS*) keys served by
    /// the JWKS endpoint. Disabled by default.
    pub allow_symmetric_jwks: bool,
}
impl<T: AsRef<str>> Debug for DecoderBuilder<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecoderBuilder")
            .field("http_client", &self.http_client)
            .field("validation", &self.validation)
            .field("audiences", &self.audiences)
            .field("allow_symmetric_jwks", &self.allow_symmetric_jwks)
            .finish()
    }
}
impl<T> DecoderBuilder<T>
where
    T: AsRef<str>,
{
    /// Create a new `DecoderBuilder`.
    #[must_use]
    pub fn new(issuer: T) -> Self {
        Self {
            issuer,
            http_client: None,
            validation: None,
            audiences: None,
            allow_symmetric_jwks: false,
        }
    }
    /// Set the http client for the decoder.
    #[must_use]
    pub fn http_client(mut self, client: HyperClient) -> Self {
        self.http_client = Some(client);
        self
    }
    /// Set the validation options for the decoder.
    ///
    /// The algorithm allowlist is honored as well. Without explicit validation
    /// options, the algorithm is selected from each trusted JWK; symmetric keys
    /// still require [`Self::allow_symmetric_jwks`].
    #[must_use]
    pub fn validation(mut self, validation: Validation) -> Self {
        self.validation = Some(validation);
        self
    }

    /// Set the expected OIDC token audience.
    #[must_use]
    pub fn audience(mut self, audience: impl Into<String>) -> Self {
        self.audiences = Some(vec![audience.into()]);
        self
    }

    /// Set the expected OIDC token audiences.
    #[must_use]
    pub fn audiences<I, S>(mut self, audiences: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.audiences = Some(audiences.into_iter().map(Into::into).collect());
        self
    }

    /// Allow symmetric (`kty: "oct"` / HS*) keys to be loaded from the
    /// JWKS endpoint.
    ///
    /// **Disabled by default.** OIDC JWKS endpoints normally publish only
    /// asymmetric public keys (RSA / EC / OKP). Accepting symmetric keys
    /// widens the trust surface — a leaked or substituted shared secret
    /// would let an attacker mint tokens the decoder accepts. Enable this
    /// only when the issuer is known to publish HS* keys and is trusted
    /// to keep them secret.
    #[must_use]
    pub fn allow_symmetric_jwks(mut self, allow: bool) -> Self {
        self.allow_symmetric_jwks = allow;
        self
    }

    /// Build a `OidcDecoder`.
    pub async fn build(self) -> Result<OidcDecoder, JwtAuthError> {
        let Self {
            issuer,
            http_client,
            validation,
            audiences,
            allow_symmetric_jwks,
        } = self;
        let raw_issuer = issuer.as_ref();

        //Create an empty JWKS to initialize our Cache
        let jwks = JwkSet { keys: Vec::new() };

        let validation = configure_oidc_validation(
            validation.unwrap_or_else(default_oidc_validation),
            raw_issuer,
            audiences,
        )?;
        let issuer = raw_issuer.to_owned();
        let cache = Arc::new(RwLock::new(
            JwkSetStore::new(jwks, CachePolicy::default(), validation)
                .with_allow_symmetric_jwks(allow_symmetric_jwks),
        ));
        let cache_state = Arc::new(CacheState::new());
        let http_client = resolve_or_else(http_client, default_http_client)?;
        let decoder = OidcDecoder {
            issuer,
            http_client,
            cache,
            cache_state,
            notifier: Arc::new(Notify::new()),
        };
        decoder.update_cache().await?;
        Ok(decoder)
    }
}

impl OidcDecoder {
    /// Create a new `OidcDecoder`.
    pub fn new(
        issuer: impl AsRef<str>,
        audience: impl Into<String>,
    ) -> impl Future<Output = Result<Self, JwtAuthError>> {
        Self::builder(issuer).audience(audience).build()
    }

    /// Create a new `DecoderBuilder`.
    pub fn builder<T>(issuer: T) -> DecoderBuilder<T>
    where
        T: AsRef<str>,
    {
        DecoderBuilder::new(issuer)
    }

    fn config_url(&self) -> String {
        format!(
            "{}/.well-known/openid-configuration",
            self.issuer.trim_end_matches('/')
        )
    }
    async fn get_config(&self) -> Result<OidcConfig, JwtAuthError> {
        let res = timeout(
            OIDC_HTTP_TIMEOUT,
            self.http_client.get(self.config_url().parse::<Uri>()?),
        )
        .await
        .map_err(|_| JwtAuthError::OidcHttpTimeout)??;
        if !res.status().is_success() {
            return Err(JwtAuthError::OidcHttpStatus(res.status()));
        }
        let body = collect_limited_body(res.into_body(), OIDC_CONFIG_MAX_BYTES).await?;
        let config: OidcConfig = serde_json::from_slice(&body)?;
        // Bind the discovered key set to the exact configured issuer. In
        // particular, normalizing a trailing slash can cross issuer boundaries.
        if config.issuer != self.issuer {
            return Err(JwtAuthError::DiscoverError);
        }
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
        let request_started = Instant::now();
        let res = timeout(OIDC_HTTP_TIMEOUT, self.http_client.get(uri))
            .await
            .map_err(|_| JwtAuthError::OidcHttpTimeout)??;
        if !res.status().is_success() {
            return Err(JwtAuthError::OidcHttpStatus(res.status()));
        }

        let cache_policy = {
            // Determine it from the cache_control header
            let cache_control = res.headers().get(CACHE_CONTROL);
            let cache_policy = CachePolicy::from_header_val(cache_control);
            Some(cache_policy)
        };
        let age = response_age(res.headers().get(AGE));
        let jwks = collect_limited_body(res.into_body(), OIDC_JWKS_MAX_BYTES).await?;

        // RFC 9111 section 4.2.3: freshness is measured from origin validation,
        // not from receipt of an already aged response from an intermediary.
        // Include transfer time so waiting for a response body adds no freshness.
        let age = age.saturating_add(request_started.elapsed().as_secs());
        let fetched_at = current_time().saturating_sub(age);
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

                if read.jwks == fetch.jwks
                    && fetch.cache_policy.unwrap_or(read.cache_policy) == read.cache_policy
                {
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
        if self
            .cache_state
            .begin_revalidation_after(OIDC_REVALIDATION_INTERVAL)
        {
            tracing::info!("Spawning Task to re-validate JWKS");
            let decoder = self.clone();
            tokio::task::spawn(async move {
                let _ = decoder.update_cache().await;
                decoder.cache_state.finish_revalidation();
                decoder.notifier.notify_waiters();
            });
        }
    }

    /// If the JWKS is currently updating in the background, this function resolves when the update is complete.
    /// If we are not currently updating the JWKS in the background, this function will resolve immediately.
    async fn wait_update(&self) {
        loop {
            let notified = self.notifier.notified();
            if !self.cache_state.is_revalidating() {
                break;
            }
            notified.await;
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
            // Try to invalidate our cache. Maybe the JWKS has changed or our cached values expired.
            // Even if it fails, stale-if-error may still let us retrieve a key.
            self.revalidate_cache();
            self.wait_update().await;
            self.get_kid(kid).await?.ok_or(JwtAuthError::CacheError)
        }
    }

    /// Gets the decoding components of a JWK by kid from the JWKS in our cache.
    /// Returns an Error, if the cache is stale and beyond the Stale While Revalidate and Stale If Error allowances configured in [`crate::cache::Settings`]
    /// Returns `Ok` if the cache is not stale.
    /// Returns `Ok` after triggering a background JWKS update if the cache is stale but within the
    /// stale-while-revalidate and stale-if-error allowances.
    #[allow(clippy::future_not_send)]
    async fn get_kid(&self, kid: &str) -> Result<Option<Arc<DecodingInfo>>, JwtAuthError> {
        let read_cache = self.cache.read().await;
        let fetched = self.cache_state.last_update();
        let max_age_secs = read_cache.cache_policy.max_age.as_secs();

        let max_age = fetched + max_age_secs;
        let now = current_time();
        let cached_key = read_cache.get_key(kid);

        if now <= max_age {
            return Ok(cached_key);
        }

        // If the stale while revalidate setting is present
        if let Some(swr) = read_cache.cache_policy.stale_while_revalidate {
            // if we're within the SWR allowed window
            if now <= swr.as_secs() + max_age {
                self.revalidate_cache();
                return Ok(cached_key);
            }
        }
        if let Some(swr_err) = read_cache.cache_policy.stale_if_error {
            // if the last update failed and the stale-if-error is present
            if now <= swr_err.as_secs() + max_age && self.cache_state.is_error() {
                self.revalidate_cache();
                return Ok(cached_key);
            }
        }
        drop(read_cache);
        tracing::info!("Returning None: {now} - {max_age}");
        Err(JwtAuthError::CacheError)
    }
}

fn default_oidc_validation() -> Validation {
    // Preserve JWK-selected algorithms for the default OIDC builder instead of
    // jsonwebtoken's HS256-only default. An explicitly supplied Validation keeps
    // its own (possibly empty) allowlist.
    Validation {
        algorithms: vec![
            Algorithm::HS256,
            Algorithm::HS384,
            Algorithm::HS512,
            Algorithm::ES256,
            Algorithm::ES384,
            Algorithm::RS256,
            Algorithm::RS384,
            Algorithm::RS512,
            Algorithm::PS256,
            Algorithm::PS384,
            Algorithm::PS512,
            Algorithm::EdDSA,
        ],
        ..Validation::default()
    }
}

fn configure_oidc_validation(
    mut validation: Validation,
    issuer: &str,
    audiences: Option<Vec<String>>,
) -> Result<Validation, JwtAuthError> {
    if validation.iss.is_none() {
        validation.set_issuer(&[issuer]);
    }
    validation.required_spec_claims.insert("iss".to_owned());

    if let Some(audiences) = audiences {
        if audiences.is_empty() {
            return Err(JwtAuthError::MissingOidcAudience);
        }
        validation.set_audience(&audiences);
        validation.validate_aud = true;
    }
    if validation.validate_aud {
        if validation.aud.is_none() {
            return Err(JwtAuthError::MissingOidcAudience);
        }
        validation.required_spec_claims.insert("aud".to_owned());
    }

    Ok(validation)
}

/// Struct used to store the computed information needed to decode a JWT
/// Intended to be cached inside of [`JwkSetStore`] to prevent decoding information about the same JWK more than once
pub struct DecodingInfo {
    // jwk: Jwk,
    key: DecodingKey,
    validation: Validation,
    // alg: Algorithm,
}
impl Debug for DecodingInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecodingInfo")
            .field("validation", &self.validation)
            .finish()
    }
}
impl DecodingInfo {
    fn new(key: DecodingKey, alg: Algorithm, validation_settings: &Validation) -> Self {
        let mut validation = validation_settings.clone();
        // A key can only validate its declared algorithm, and only when that
        // algorithm is permitted by the caller's validation policy.
        validation.algorithms.retain(|allowed| *allowed == alg);

        Self {
            // jwk,
            key,
            validation,
            // alg,
        }
    }

    fn decode<T>(&self, token: &str) -> Result<TokenData<T>, JwtAuthError>
    where
        T: for<'de> serde::de::Deserialize<'de> + Clone,
    {
        match jsonwebtoken::decode::<T>(token, &self.key, &self.validation) {
            Ok(data) => Ok(data),
            Err(e) => {
                tracing::error!(error = ?e, "error decoding jwt token");
                Err(JwtAuthError::from(e))
            }
        }
    }
}

/// Helper Struct that contains the response of a request to the jwks uri
/// `cache_policy` will be Some when [`cache::Strategy`] is set to [`cache::Strategy::Automatic`].
#[derive(Debug)]
pub(crate) struct JwkSetFetch {
    jwks: JwkSet,
    cache_policy: Option<CachePolicy>,
    fetched_at: u64,
}

#[derive(Debug, Deserialize)]
struct OidcConfig {
    issuer: String,
    jwks_uri: String,
}

async fn collect_limited_body(
    body: salvo_core::hyper::body::Incoming,
    max_size: usize,
) -> Result<Bytes, JwtAuthError> {
    let limited = Limited::new(body, max_size);
    let collected = timeout(OIDC_HTTP_TIMEOUT, limited.collect())
        .await
        .map_err(|_| JwtAuthError::OidcHttpTimeout)?
        .map_err(|error| {
            if error.is::<http_body_util::LengthLimitError>() {
                JwtAuthError::OidcResponseTooLarge(max_size)
            } else {
                JwtAuthError::OidcBodyRead(error)
            }
        })?;
    Ok(collected.to_bytes())
}

pub(crate) fn decode_jwk(
    jwk: &Jwk,
    validation: &Validation,
    allow_symmetric_jwks: bool,
) -> Result<(String, DecodingInfo), JwtAuthError> {
    if jwk
        .common
        .public_key_use
        .as_ref()
        .is_some_and(|usage| *usage != PublicKeyUse::Signature)
        || jwk.common.key_operations.as_ref().is_some_and(|operations| {
            !operations.contains(&KeyOperations::Verify)
                || operations
                    .iter()
                    .any(|operation| !matches!(operation, KeyOperations::Sign | KeyOperations::Verify))
        })
    {
        return Err(JwtAuthError::InvalidJwk);
    }
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
            if !allow_symmetric_jwks {
                tracing::warn!(
                    kid = ?kid,
                    "rejecting symmetric (oct) JWK; OIDC JWKS should publish public keys. \
                     Enable allow_symmetric_jwks(true) on the decoder builder to opt in."
                );
                return Err(JwtAuthError::InvalidJwk);
            }
            DecodingKey::from_base64_secret(&params.value).ok()
        }
        jsonwebtoken::jwk::AlgorithmParameters::OctetKeyPair(ref params) => {
            let der = b64_decode(&params.x)?;

            Some(DecodingKey::from_ed_der(&der))
        }
        _ => None,
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

fn response_age(value: Option<&HeaderValue>) -> u64 {
    // Age is a non-negative integer. Use only the first list member if an
    // intermediary combined duplicate fields (RFC 9111 section 5.1).
    let Some(value) = value.and_then(|value| value.as_bytes().split(|byte| *byte == b',').next()) else {
        return 0;
    };
    let value = value.trim_ascii();
    if value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
        return 0;
    }
    // An overflowing age means very old, rather than an apparently fresh zero.
    value.iter().fold(0u64, |age, byte| age.saturating_mul(10).saturating_add(u64::from(byte - b'0')))
}

pub(crate) fn current_time() -> u64 {
    // Called on the token-validation hot path; treat a clock set before the epoch
    // (NTP step / VM time travel) as `0` rather than panicking.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use hyper_util::rt::TokioIo;
    use salvo_core::hyper::{Response, server::conn::http1, service::service_fn};
    use serde_json::json;

    use super::*;

    fn test_http_client() -> HyperClient {
        let tls = rustls::ClientConfig::builder_with_provider(default_rustls_crypto_provider())
            .with_safe_default_protocol_versions().unwrap()
            .with_root_certificates(rustls::RootCertStore::empty()).with_no_client_auth();
        let connector = HttpsConnectorBuilder::new().with_tls_config(tls)
            .https_or_http().enable_http1().build();
        Client::builder(TokioExecutor::new()).build(connector)
    }

    #[test]
    fn response_age_uses_first_delta_seconds_and_saturates_overflow() {
        assert_eq!(response_age(None), 0);
        for (value, expected) in [
            ("0", 0), ("3600", 3600), (" 3599 \t", 3599), ("12, 999", 12),
            ("+12", 0), ("-12", 0), ("1.2", 0), ("unknown", 0), ("", 0),
            ("18446744073709551615", u64::MAX),
            ("18446744073709551616", u64::MAX),
            ("99999999999999999999999999999999", u64::MAX),
        ] {
            assert_eq!(response_age(Some(&HeaderValue::from_str(value).unwrap())), expected, "{value}");
        }
    }

    #[tokio::test]
    async fn quoted_cache_extensions_do_not_delay_jwks_key_revocation() {
        let _ = crate::install_crypto_provider();
        struct ServerTask(tokio::task::JoinHandle<()>);
        impl Drop for ServerTask {
            fn drop(&mut self) {
                self.0.abort();
            }
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let discovery = json!({"issuer":issuer,"jwks_uri":format!("{issuer}/jwks")}).to_string();
        let revoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fetches = Arc::new(AtomicUsize::new(0));
        let (server_revoked, server_fetches) = (revoked.clone(), fetches.clone());
        let _server = ServerTask(tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let (revoked, fetches, discovery) = (
                    server_revoked.clone(),
                    server_fetches.clone(),
                    discovery.clone(),
                );
                tokio::spawn(async move {
                    let service = service_fn(move |req: salvo_core::hyper::Request<_>| {
                        let body = if req.uri().path() == "/jwks" {
                            fetches.fetch_add(1, Ordering::SeqCst);
                            if revoked.load(Ordering::SeqCst) {
                                json!({"keys":[]})
                            } else {
                                json!({"keys":[{"kty":"oct","kid":"initial","alg":"HS256","k":"c2VjcmV0"}]})
                            }.to_string()
                        } else {
                            discovery.clone()
                        };
                        async move {
                            Ok::<_, Infallible>(Response::builder()
                                .header(CACHE_CONTROL, "max-age=1, stale-while-revalidate=0, extension=\"ignored,max-age=2592000,ignored\"")
                                .body(Full::new(Bytes::from(body))).unwrap())
                        }
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        }));

        let mut header = jsonwebtoken::Header::new(Algorithm::HS256);
        header.kid = Some("initial".into());
        let token = jsonwebtoken::encode(
            &header,
            &json!({"iss":issuer,"aud":"test-client","exp":current_time()+3600}),
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        let decoder = OidcDecoder::builder(issuer)
            .audience("test-client")
            .allow_symmetric_jwks(true)
            .http_client(test_http_client())
            .build()
            .await
            .unwrap();
        assert!(
            decoder
                .decode::<serde_json::Value>(&token, &mut Depot::new())
                .await
                .is_ok()
        );
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
        revoked.store(true, Ordering::SeqCst);
        // Real elapsed time exceeds the trusted endpoint's one-second policy
        // even with the cache's whole-second clock granularity.
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(
            decoder
                .decode::<serde_json::Value>(&token, &mut Depot::new())
                .await
                .is_err(),
            "revoked key remained usable after the endpoint's max-age"
        );
        assert_eq!(fetches.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn discovery_issuer_is_required_and_must_match_before_fetching_jwks() {
        for mode in ["mismatch", "missing", "missing-slash", "extra-slash", "match", "tenant"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let configured = match mode {
                "missing-slash" => format!("{base}/"),
                "tenant" => format!("{base}/tenant/"),
                _ => base.clone(),
            };
            let mut metadata = json!({"jwks_uri": format!("{base}/jwks")});
            if mode != "missing" {
                metadata["issuer"] = json!(match mode {
                    "mismatch" => format!("{base}/other-tenant"),
                    "missing-slash" => base.clone(),
                    "extra-slash" => format!("{base}/"),
                    _ => configured.clone(),
                });
            }
            let metadata = metadata.to_string();
            let fetches = Arc::new(AtomicUsize::new(0));
            let server_fetches = Arc::clone(&fetches);
            let server = tokio::spawn(async move {
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    let metadata = metadata.clone();
                    let fetches = Arc::clone(&server_fetches);
                    tokio::spawn(async move {
                        let service = service_fn(move |req: salvo_core::hyper::Request<_>| {
                            let body = if req.uri().path() == "/jwks" {
                                fetches.fetch_add(1, Ordering::Relaxed);
                                r#"{"keys":[]}"#.to_owned()
                            } else { metadata.clone() };
                            async move { Ok::<_, Infallible>(Response::new(Full::new(Bytes::from(body)))) }
                        });
                        let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
                    });
                }
            });
            let result = OidcDecoder::builder(&configured).audience("test-client")
                .http_client(test_http_client()).build().await;
            server.abort();
            let expected = matches!(mode, "match" | "tenant");
            assert_eq!(result.is_ok(), expected, "discovery mode {mode}");
            assert_eq!(fetches.load(Ordering::Relaxed), usize::from(expected), "discovery mode {mode}");
        }
    }

    #[test]
    fn jwk_rejects_explicit_non_verification_purposes() {
        for usage in [
            json!({"use":"enc"}), json!({"use":"unknown"}),
            json!({"key_ops":[]}), json!({"key_ops":["encrypt"]}),
            json!({"key_ops":["sign"]}), json!({"key_ops":["verify", "encrypt"]}),
            json!({"use":"enc", "key_ops":["verify"]}),
            json!({"use":"sig", "key_ops":["encrypt"]}),
        ] {
            let mut key = json!({"kty":"oct", "kid":"test", "alg":"HS256", "k":"c2VjcmV0"});
            key.as_object_mut().unwrap().extend(usage.as_object().unwrap().clone());
            let jwk: Jwk = serde_json::from_value(key).unwrap();
            assert!(matches!(decode_jwk(&jwk, &Validation::new(Algorithm::HS256), true), Err(JwtAuthError::InvalidJwk)), "purpose {usage}");
        }
    }

    #[test]
    fn jwk_accepts_missing_or_consistent_verification_purposes() {
        let _ = crate::install_crypto_provider();
        let token = jsonwebtoken::encode(&jsonwebtoken::Header::new(Algorithm::HS256),
            &json!({"exp": current_time() + 3600}), &jsonwebtoken::EncodingKey::from_secret(b"secret")).unwrap();
        for usage in [json!({}), json!({"use":"sig"}), json!({"key_ops":["verify"]}),
            json!({"key_ops":["sign", "verify"]}), json!({"use":"sig", "key_ops":["verify"]})] {
            let mut key = json!({"kty":"oct", "kid":"test", "alg":"HS256", "k":"c2VjcmV0"});
            key.as_object_mut().unwrap().extend(usage.as_object().unwrap().clone());
            let jwk: Jwk = serde_json::from_value(key).unwrap();
            let (_, info) = decode_jwk(&jwk, &Validation::new(Algorithm::HS256), true).unwrap();
            assert!(info.decode::<serde_json::Value>(&token).is_ok(), "purpose {usage}");
        }
    }

    #[test]
    fn refreshed_jwks_excludes_encryption_keys_and_removes_revoked_purposes() {
        let mut store = JwkSetStore::new(JwkSet { keys: vec![] }, CachePolicy::default(), Validation::new(Algorithm::HS256))
            .with_allow_symmetric_jwks(true);
        let signing = json!({"kty":"oct", "kid":"signing", "alg":"HS256", "k":"c2VjcmV0", "use":"sig", "key_ops":["verify"]});
        let encryption = json!({"kty":"oct", "kid":"encryption", "alg":"HS256", "k":"c2VjcmV0", "use":"enc", "key_ops":["encrypt"]});
        store.update_fetch(JwkSetFetch { jwks: serde_json::from_value(json!({"keys":[signing.clone(), encryption]})).unwrap(),
            cache_policy: Some(CachePolicy::default()), fetched_at: current_time() });
        assert!(store.get_key("signing").is_some());
        assert!(store.get_key("encryption").is_none());
        let mut revoked = signing;
        revoked["use"] = json!("enc");
        revoked["key_ops"] = json!(["encrypt"]);
        store.update_fetch(JwkSetFetch { jwks: serde_json::from_value(json!({"keys":[revoked]})).unwrap(),
            cache_policy: Some(CachePolicy::default()), fetched_at: current_time() });
        assert!(store.get_key("signing").is_none());
    }

    #[test]
    fn explicit_algorithm_policy_rejects_an_otherwise_valid_signature() {
        let _ = crate::install_crypto_provider();
        let secret = b"test-only-secret";
        let claims = json!({"exp": current_time() + 3600});
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(Algorithm::HS512),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(secret),
        )
        .unwrap();

        let denied = DecodingInfo::new(
            DecodingKey::from_secret(secret),
            Algorithm::HS512,
            &Validation::new(Algorithm::HS256),
        );
        assert!(denied.decode::<serde_json::Value>(&token).is_err());

        let allowed = DecodingInfo::new(
            DecodingKey::from_secret(secret),
            Algorithm::HS512,
            &Validation::new(Algorithm::HS512),
        );
        assert!(allowed.decode::<serde_json::Value>(&token).is_ok());

        let default = DecodingInfo::new(
            DecodingKey::from_secret(secret),
            Algorithm::HS512,
            &default_oidc_validation(),
        );
        assert!(default.decode::<serde_json::Value>(&token).is_ok());

        let mut validation = Validation::new(Algorithm::HS512);
        validation.algorithms.clear();
        let info = DecodingInfo::new(
            DecodingKey::from_secret(secret),
            Algorithm::HS512,
            &validation,
        );
        assert!(info.decode::<serde_json::Value>(&token).is_err());
    }

    #[tokio::test]
    async fn unknown_kids_share_a_cooldown_and_rotation_remains_possible() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let discovery = format!(r#"{{"issuer":"{issuer}","jwks_uri":"{issuer}/jwks"}}"#);
        let keys = Arc::new(std::sync::RwLock::new(
            json!({"keys": [{
                "kty": "oct", "kid": "initial", "alg": "HS256", "k": "c2VjcmV0"
            }]})
            .to_string(),
        ));
        let requests = Arc::new(AtomicUsize::new(0));
        let server_keys = Arc::clone(&keys);
        let server_requests = Arc::clone(&requests);
        let server = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let keys = Arc::clone(&server_keys);
                let requests = Arc::clone(&server_requests);
                let discovery = discovery.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |req: salvo_core::hyper::Request<_>| {
                        let body = if req.uri().path() == "/jwks" {
                            requests.fetch_add(1, Ordering::Relaxed);
                            keys.read().unwrap().clone()
                        } else {
                            discovery.clone()
                        };
                        async move {
                            Ok::<_, Infallible>(
                                Response::builder()
                                    .status(if body.is_empty() {
                                        salvo_core::http::StatusCode::SERVICE_UNAVAILABLE
                                    } else {
                                        salvo_core::http::StatusCode::OK
                                    })
                                    .header(CACHE_CONTROL, "max-age=3600")
                                    .body(Full::new(Bytes::from(body)))
                                    .unwrap(),
                            )
                        }
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });

        let tls = rustls::ClientConfig::builder_with_provider(default_rustls_crypto_provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();
        let connector = HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(connector);
        let decoder = OidcDecoder::builder(issuer)
            .audience("test-client")
            .allow_symmetric_jwks(true)
            .http_client(client)
            .build()
            .await
            .unwrap();
        assert!(decoder.get_kid_retry("initial").await.is_ok());
        for kid in ["missing-a", "missing-b", "missing-c"] {
            assert!(decoder.get_kid_retry(kid).await.is_err());
        }
        // One initial fetch and one early refresh, regardless of the number of
        // distinct misses or clones of the decoder.
        assert!(decoder.clone().get_kid_retry("missing-d").await.is_err());
        assert_eq!(requests.load(Ordering::Relaxed), 2);

        *keys.write().unwrap() = json!({"keys": [{
            "kty": "oct", "kid": "rotated", "alg": "HS256", "k": "c2VjcmV0"
        }]})
        .to_string();
        tokio::time::sleep(OIDC_REVALIDATION_INTERVAL + Duration::from_millis(20)).await;
        assert!(decoder.get_kid_retry("rotated").await.is_ok());
        assert_eq!(requests.load(Ordering::Relaxed), 3);

        // Failed refreshes are rate-limited too, while already cached keys keep
        // working throughout their advertised freshness lifetime.
        keys.write().unwrap().clear();
        tokio::time::sleep(OIDC_REVALIDATION_INTERVAL + Duration::from_millis(20)).await;
        for kid in ["missing-e", "missing-f", "missing-g"] {
            assert!(decoder.get_kid_retry(kid).await.is_err());
        }
        assert_eq!(requests.load(Ordering::Relaxed), 4);
        assert!(decoder.get_kid_retry("rotated").await.is_ok());
        server.abort();
    }

    #[test]
    fn test_uses_provided_http_client_without_building_default_client() {
        let resolved =
            resolve_or_else(Some("provided-client"), || -> Result<&str, JwtAuthError> {
                panic!("default HTTP client should not be built when a custom client is supplied")
            })
            .unwrap();

        assert_eq!(resolved, "provided-client");
    }

    #[test]
    fn test_decode_jwk_missing_alg() {
        let jwk_json = json!({
            "kty": "RSA",
            "kid": "test-rsa",
            "n": "...",
            "e": "AQAB"
        });
        let jwk: Jwk = serde_json::from_value(jwk_json).unwrap();
        let validation = Validation::new(Algorithm::RS256);
        let result = decode_jwk(&jwk, &validation, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_jwk_rejects_symmetric_by_default() {
        // valid base64url of "secret"
        let jwk_json = json!({
            "kty": "oct",
            "kid": "test-oct",
            "alg": "HS256",
            "k": "c2VjcmV0"
        });
        let jwk: Jwk = serde_json::from_value(jwk_json).unwrap();
        let validation = Validation::new(Algorithm::HS256);
        let result = decode_jwk(&jwk, &validation, false);
        assert!(matches!(result, Err(JwtAuthError::InvalidJwk)));
    }

    #[test]
    fn test_decode_jwk_accepts_symmetric_when_opted_in() {
        let jwk_json = json!({
            "kty": "oct",
            "kid": "test-oct",
            "alg": "HS256",
            "k": "c2VjcmV0"
        });
        let jwk: Jwk = serde_json::from_value(jwk_json).unwrap();
        let validation = Validation::new(Algorithm::HS256);
        let result = decode_jwk(&jwk, &validation, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_decode_jwk_rejects_unknown_key_type() {
        let jwk_json = json!({
            "kty": "unknown",
            "kid": "test-unknown",
            "alg": "RS256"
        });
        let jwk: Jwk = serde_json::from_value(jwk_json).unwrap();
        let validation = Validation::new(Algorithm::RS256);
        let result = decode_jwk(&jwk, &validation, false);
        assert!(matches!(result, Err(JwtAuthError::InvalidJwk)));
    }

    #[test]
    fn test_oidc_validation_requires_audience() {
        let result =
            configure_oidc_validation(Validation::default(), "https://issuer.example", None);

        assert!(matches!(result, Err(JwtAuthError::MissingOidcAudience)));
    }

    #[test]
    fn test_oidc_validation_sets_issuer_and_audience_claims() {
        let validation = configure_oidc_validation(
            Validation::default(),
            "https://issuer.example",
            Some(vec!["client-id".to_owned()]),
        )
        .unwrap();

        assert!(
            validation
                .iss
                .as_ref()
                .is_some_and(|iss| iss.contains("https://issuer.example"))
        );
        assert!(
            validation
                .aud
                .as_ref()
                .is_some_and(|aud| aud.contains("client-id"))
        );
        assert!(validation.required_spec_claims.contains("iss"));
        assert!(validation.required_spec_claims.contains("aud"));
    }

    #[test]
    fn test_oidc_validation_preserves_trailing_slash_issuer() {
        let validation = configure_oidc_validation(
            Validation::default(),
            "https://issuer.example/",
            Some(vec!["client-id".to_owned()]),
        )
        .unwrap();

        assert!(
            validation
                .iss
                .as_ref()
                .is_some_and(|iss| iss.contains("https://issuer.example/"))
        );
        assert!(
            validation
                .iss
                .as_ref()
                .is_some_and(|iss| !iss.contains("https://issuer.example"))
        );
    }
}
