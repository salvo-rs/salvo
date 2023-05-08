//! ACME supports.
//!
//! Reference: <https://datatracker.ietf.org/doc/html/rfc8555>
//! Reference: <https://datatracker.ietf.org/doc/html/rfc8737>
//!
//! * HTTP-01
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello World"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut router = Router::new().get(hello);
//!     let listener = TcpListener::new("0.0.0.0:443")
//!         .acme()
//!         // .directory("letsencrypt", salvo::conn::acme::LETS_ENCRYPT_STAGING)
//!         .cache_path("acme/letsencrypt")
//!         .add_domain("acme-http01.salvo.rs")
//!         .http01_challege(&mut router);
//!     let acceptor = listener.join(TcpListener::new("0.0.0.0:80")).bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
//!
//! * TLS ALPN-01
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello World"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new().get(hello);
//!     let acceptor = TcpListener::new("0.0.0.0:443")
//!         .acme()
//!         // .directory("letsencrypt", salvo::conn::acme::LETS_ENCRYPT_STAGING)
//!         .cache_path("acme/letsencrypt")
//!         .add_domain("acme-tls-alpn01.salvo.rs")
//!         .bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```

pub mod cache;
mod client;
mod config;
mod issuer;
mod jose;
mod key_pair;
mod listener;
mod resolver;

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::sync::{ Arc};

use client::AcmeClient;
use serde::{Deserialize, Serialize};
use http_body_util::Full;
use bytes::Bytes;
use parking_lot::RwLock;

use crate::http::StatusError;
use crate::{async_trait, Depot, FlowCtrl, Handler, Request, Response};
use cache::AcmeCache;
pub use config::{AcmeConfig, AcmeConfigBuilder};
pub use listener::AcmeListener;

/// Letsencrypt production directory url
pub const LETS_ENCRYPT_PRODUCTION: &str = "https://acme-v02.api.letsencrypt.org/directory";
/// Letsencrypt staging directory url
pub const LETS_ENCRYPT_STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";

/// Well known acme challenge path
pub(crate) const WELL_KNOWN_PATH: &str = "/.well-known/acme-challenge";

/// HTTP-01 challenge
const CHALLENGE_TYPE_HTTP_01: &str = "http-01";

/// TLS-ALPN-01 challenge
const CHALLENGE_TYPE_TLS_ALPN_01: &str = "tls-alpn-01";

/// Challenge type
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChallengeType {
    /// HTTP-01 challenge
    ///
    /// Reference: <https://letsencrypt.org/docs/challenge-types/#http-01-challenge>
    Http01,
    /// TLS-ALPN-01
    ///
    /// Reference: <https://letsencrypt.org/docs/challenge-types/#tls-alpn-01>
    TlsAlpn01,
}
impl Display for ChallengeType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ChallengeType::Http01 => f.write_str(CHALLENGE_TYPE_HTTP_01),
            ChallengeType::TlsAlpn01 => f.write_str(CHALLENGE_TYPE_TLS_ALPN_01),
        }
    }
}

pub(crate) type FullBody = Full<Bytes>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Directory {
    pub(crate) new_nonce: String,
    pub(crate) new_account: String,
    pub(crate) new_order: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Identifier {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Problem {
    pub(crate) detail: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Challenge {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) url: String,
    pub(crate) token: String,
}

/// Handler for `HTTP-01` challenge.
pub(crate) struct Http01Handler {
    pub(crate) keys: Arc<RwLock<HashMap<String, String>>>,
}

#[async_trait]
impl Handler for Http01Handler {
    #[inline]
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        if let Some(token) = req.params().get("token") {
            let keys = self.keys.read();
            if let Some(value) = keys.get(token) {
                res.render(value);
            } else {
                tracing::error!(token = %token, "keys not found for token");
                res.render(token);
            }
        } else {
            res.render(StatusError::not_found().summary("token is not provide"));
        }
    }
}
