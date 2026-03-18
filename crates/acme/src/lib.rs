//! Automatic HTTPS/TLS certificate management for Salvo via the ACME protocol.
//!
//! This crate integrates [certon](https://crates.io/crates/certon) — a
//! production-grade ACME client — with Salvo's listener/acceptor system.
//!
//! ## Features
//!
//! - **Multiple issuers**: Let's Encrypt, ZeroSSL, or any ACME-compatible CA.
//! - **Multiple challenge types**: HTTP-01, TLS-ALPN-01, DNS-01.
//! - **On-demand TLS**: obtain certificates at handshake time.
//! - **OCSP stapling**: automatic OCSP response fetching and stapling.
//! - **Multiple key types**: ECDSA P-256/P-384/P-521, RSA, Ed25519.
//! - **Persistent storage**: pluggable storage backend via [`certon::Storage`].
//! - **Background renewal**: automatic certificate renewal and OCSP refresh.
//!
//! ## Quick Start — HTTP-01
//!
//! ```ignore
//! use salvo_acme::AcmeListener;
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
//!         .cache_path("acme/letsencrypt")
//!         .add_domain("example.com")
//!         .http01_challenge(&mut router);
//!     let acceptor = listener.join(TcpListener::new("0.0.0.0:80")).bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
//!
//! ## Quick Start — TLS-ALPN-01
//!
//! ```ignore
//! use salvo_acme::AcmeListener;
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
//!         .cache_path("acme/letsencrypt")
//!         .add_domain("example.com")
//!         .bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```

#[macro_use]
mod cfg;

mod config;
mod listener;

use std::collections::HashMap;
use std::sync::Arc;

pub use config::{AcmeConfig, AcmeConfigBuilder};
pub use listener::{AcmeAcceptor, AcmeListenerBuilder};
use salvo_core::conn::tcp::TcpListener;
use salvo_core::http::StatusError;
use salvo_core::{Depot, FlowCtrl, Handler, Request, Response, async_trait};
use tokio::net::ToSocketAddrs;
use tokio::sync::RwLock;

cfg_feature! {
    #![feature = "quinn"]
    pub use listener::AcmeQuinnListener;
}

// ---------------------------------------------------------------------------
// Re-exports from certon for advanced usage
// ---------------------------------------------------------------------------

/// Re-export the entire `certon` crate for advanced configuration.
pub use certon;

pub use certon::{
    AcmeIssuer, AcmeIssuerBuilder, CertCache, CertIssuer, CertResolver, Certificate,
    Config as CertonConfig, ConfigBuilder as CertonConfigBuilder, DnsProvider,
    Dns01Solver, DistributedSolver, FileStorage, Http01Solver, IssuedCertificate,
    IssuerPolicy, KeyType, Manager, MaintenanceConfig, OcspConfig, OnDemandConfig,
    PreChecker, Revoker, Solver, Storage, TlsAlpn01Solver, ZeroSslIssuer,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Let's Encrypt production directory URL.
pub const LETS_ENCRYPT_PRODUCTION: &str = certon::LETS_ENCRYPT_PRODUCTION;
/// Let's Encrypt staging directory URL.
pub const LETS_ENCRYPT_STAGING: &str = certon::LETS_ENCRYPT_STAGING;
/// ZeroSSL production directory URL.
pub const ZEROSSL_PRODUCTION: &str = certon::ZEROSSL_PRODUCTION;

/// Well known ACME challenge path.
pub(crate) const WELL_KNOWN_PATH: &str = "/.well-known/acme-challenge";

/// Challenge type for ACME.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChallengeType {
    /// HTTP-01 challenge.
    ///
    /// Reference: <https://letsencrypt.org/docs/challenge-types/#http-01-challenge>
    Http01,
    /// TLS-ALPN-01 challenge.
    ///
    /// Reference: <https://letsencrypt.org/docs/challenge-types/#tls-alpn-01>
    TlsAlpn01,
    /// DNS-01 challenge.
    ///
    /// Reference: <https://letsencrypt.org/docs/challenge-types/#dns-01-challenge>
    Dns01,
}

// ---------------------------------------------------------------------------
// HTTP-01 challenge handler (Salvo Handler implementation)
// ---------------------------------------------------------------------------

/// Handler for HTTP-01 ACME challenges.
///
/// Reads challenge tokens from a shared map that is populated by the ACME
/// issuance flow. This handler should be registered on the router at
/// `/.well-known/acme-challenge/{token}`.
pub struct Http01Handler {
    pub(crate) keys: Arc<RwLock<HashMap<String, String>>>,
}
impl std::fmt::Debug for Http01Handler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Http01Handler").finish()
    }
}

#[async_trait]
impl Handler for Http01Handler {
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        if let Some(token) = req.params().get("token") {
            // First check our local map.
            let keys = self.keys.read().await;
            if let Some(value) = keys.get(token) {
                res.render(value);
                return;
            }
            drop(keys);

            // Fall back to certon's global active challenge map.
            if let Some(value) = certon::solvers::get_active_challenge(token) {
                res.render(value);
                return;
            }

            tracing::error!(token, "key not found for ACME challenge token");
            res.render(token);
        } else {
            res.render(StatusError::not_found().brief("Token is not provided."));
        }
    }
}

// ---------------------------------------------------------------------------
// ListenerAcmeExt
// ---------------------------------------------------------------------------

/// Extension trait for Listener to support ACME.
pub trait AcmeListener {
    /// Enable ACME support for the listener.
    fn acme(self) -> AcmeListenerBuilder<Self> where Self: Sized;
}

impl<T> AcmeListener for TcpListener<T>
where
    T: ToSocketAddrs + Send + 'static,
{
    fn acme(self) -> AcmeListenerBuilder<Self> {
        AcmeListenerBuilder::new(self)
    }
}
