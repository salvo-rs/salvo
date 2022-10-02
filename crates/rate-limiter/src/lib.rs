//! TBD
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::error::Error as StdError;
use std::hash::Hash;

use salvo_core::addr::SocketAddr;
use salvo_core::handler::Skipper;
use salvo_core::http::{Request, Response, StatusCode, StatusError};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "memory-store"]

    mod memory_store;
    pub use memory_store::MemoryStore;
}

cfg_feature! {
    #![feature = "fixed-window"]

    mod fixed_window;
    pub use fixed_window::FixedWindow;
}
cfg_feature! {
    #![feature = "sliding-window"]

    mod sliding_window;
    pub use sliding_window::SlidingWindow;
}
cfg_feature! {
    #![feature = "leaky-bucket"]

    mod leaky_bucket;
    pub use leaky_bucket::LeakyBucket;
}
cfg_feature! {
    #![feature = "token-bucket"]

    mod token_bucket;
    pub use token_bucket::TokenBucket;
}

/// Identifer
pub trait Identifer: Send + Sync + 'static {
    type Key: Hash + Eq + Send + Clone + 'static;
    /// Get the identifier.
    fn get(&self, req: &mut Request, depot: &Depot) -> Option<Self::Key>;
}
impl<F, K> Identifer for F
where
    F: Fn(&mut Request, &Depot) -> Option<K> + Send + Sync + 'static,
    K: Send + Eq + Hash + Clone + 'static,
{
    type Key = K;
    fn get(&self, req: &mut Request, depot: &Depot) -> Option<Self::Key> {
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
pub trait RateStrategy: Clone + Send + Sync + 'static {
    async fn check(&mut self) -> bool;
}

/// Store rate.
#[async_trait]
pub trait RateStore: Send + Sync + 'static {
    /// Error type for RateStore.
    type Error: StdError + Send + Sync + 'static;
    /// Key
    type Key: Hash + Eq + Send + Clone + 'static;
    /// Saved strategy.
    type Strategy;
    /// Get the strategy from the store.
    async fn load_strategy(&self, key: &Self::Key, config: &Self::Strategy) -> Result<Self::Strategy, Self::Error>;
    // Save the strategy from the store.
    async fn save_strategy(&self, key: Self::Key, strategy: Self::Strategy) -> Result<(), Self::Error>;
}

/// RateLimiter
pub struct RateLimiter<G, S, I> {
    strategy: G,
    store: S,
    identifier: I,
    skipper: Option<Box<dyn Skipper>>,
}

impl<G: RateStrategy, S: RateStore, I: Identifer> RateLimiter<G, S, I> {
    /// Create a new RateLimiter
    #[inline]
    pub fn new(strategy: G, store: S, identifier: I) -> Self {
        Self {
            strategy,
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
impl<G, S, I> Handler for RateLimiter<G, S, I>
where
    G: RateStrategy,
    S: RateStore<Key = I::Key, Strategy = G>,
    I: Identifer,
    I::Key: Send + Sync + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if let Some(skipper) = &self.skipper {
            if skipper.skipped(req, depot) {
                return;
            }
        }
        let identifier = self.identifier.get(req, depot);
        if let Some(identifier) = identifier {
            let strategy = {
                let key = identifier.clone();
                self.store.load_strategy(&key, &self.strategy).await
            };
            match strategy {
                Ok(mut strategy) => {
                    let allowed = strategy.check().await;
                    if !allowed {
                        res.set_status_code(StatusCode::TOO_MANY_REQUESTS);
                        ctrl.skip_rest();
                    }
                    if let Err(e) = self.store.save_strategy(identifier, strategy).await {
                        tracing::error!(error = ?e, "RateLimiter save strategy failed");
                    }
                }
                Err(e) => {
                    tracing::error!(error = ?e, "RateLimiter error");
                    res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    ctrl.skip_rest();
                }
            }
        } else {
            res.set_status_error(StatusError::bad_request().with_detail("invalid identifier"));
            ctrl.skip_rest();
        }
    }
}
