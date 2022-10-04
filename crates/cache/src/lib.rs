//! TBD
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// A constructed via `salvo_cache::Cache::builder()`.
#[derive(Clone, Debug)]
pub struct Cache {
}

impl Cache {
    /// Create new `Cache`.
    #[inline]
    pub fn new() -> Self {
        Cache {
        }
    }
}
