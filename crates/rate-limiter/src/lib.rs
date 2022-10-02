//! TBD
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::error::Error as StdError;

use salvo_core::addr::SocketAddr;
use salvo_core::handler::Skipper;
use salvo_core::http::{Request, Response, StatusCode, StatusError};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};
use serde::de::DeserializeOwned;
use serde::{Serialize};

/// Identifer
pub trait Identifer: Send + Sync + 'static {
    /// Get the identifier.
    fn get(&self, req: &mut Request, depot: &Depot) -> Option<String>;
}
impl<F> Identifer for F
where
    F: Fn(&mut Request, &Depot) -> Option<String> + Send + Sync + 'static,
{
    fn get(&self, req: &mut Request, depot: &Depot) -> Option<String> {
        (self)(req, depot)
    }
}

/// Use request ip as identifier.
pub fn real_ip_identifer(req: &mut Request, _depot: &Depot) -> Option<String> {
    match req.remote_addr() {
        Some(SocketAddr::IPv4(addr)) => Some(addr.ip().to_string()),
        Some(SocketAddr::IPv6(addr)) => Some(addr.ip().to_string()),
        Some(_) => None,
        None => None,
    }
}

/// RateStrategy
#[async_trait]
pub trait RateStrategy: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Error
    type Error: StdError + Send + Sync + 'static;
    /// Check if the request is allowed.
    fn allow(&self, identifer: &str) -> Result<bool, Self::Error>;
}

/// Store rate.
#[async_trait]
pub trait RateStore: Send + Sync + 'static {
    /// Error type for RateStore.
    type Error: std::error::Error + Send + Sync + 'static;
    /// Saved strategy.
    type Strategy: RateStrategy;
    /// Get the strategy from the store.
    async fn load_strategy(&self, identifer: &str) -> Result<Self::Strategy, Self::Error>;
    /// Save the strategy from the store.
    async fn save_strategy(&self, identifer: &str, strategy: Self::Strategy) -> Result<(), Self::Error>;
}

/// RateLimiter
pub struct RateLimiter<S, I> {
    store: S,
    identifier: I,
    skipper: Option<Box<dyn Skipper>>,
}

impl<S: RateStore, I: Identifer> RateLimiter<S, I> {
    /// Create a new RateLimiter
    #[inline]
    pub fn new(store: S, identifier: I) -> Self {
        Self {
            store,
            identifier,
            skipper: None,
        }
    }

    /// Set skipper and returns new `RateLimiter`.
    #[inline]
    pub fn with_skipper(mut self, skipper: impl Skipper) -> Self {
        self.skipper = Some(Box::new(skipper));
        self
    }
}

#[async_trait]
impl<S, I> Handler for RateLimiter<S, I>
where
    S: RateStore,
    I: Identifer,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if let Some(skipper) = &self.skipper {
            if skipper.skipped(req, depot) {
                return;
            }
        }
        let identifier = self.identifier.get(req, depot);
        if let Some(identifier) = identifier {
            let strategy = self
                .store
                .load_strategy(&identifier)
                .await
                .expect("rate limit strategy should loaded from store");
            if !strategy.allow(&identifier).unwrap_or(false) {
                res.set_status_code(StatusCode::TOO_MANY_REQUESTS);
                ctrl.skip_rest();
            }
        } else {
            res.set_status_error(StatusError::bad_request().with_detail("invalid identifier"));
            ctrl.skip_rest();
        }
    }
}
