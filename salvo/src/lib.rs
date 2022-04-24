//! Salvo is a powerful and simplest web server framework in Rust world.
//! Read more: <https://salvo.rs>

#![doc(html_favicon_url = "https://salvo.rs/images/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
pub use salvo_core as core;
pub use salvo_core::*;

#[cfg(any(
    feature = "extra",
    feature = "basic_auth",
    feature = "compression",
    feature = "cors",
    feature = "csrf",
    feature = "jwt-auth",
    feature = "logging",
    feature = "proxy",
    feature = "serve-static",
    feature = "session",
    feature = "size-limiter",
    feature = "sse",
    feature = "timeout",
    feature = "ws"
))]
#[cfg_attr(
    docsrs,
    doc(cfg(any(
        feature = "extra",
        feature = "basic_auth",
        feature = "compression",
        feature = "cors",
        feature = "csrf",
        feature = "jwt-auth",
        feature = "logging",
        feature = "proxy",
        feature = "serve-static",
        feature = "session",
        feature = "size-limiter",
        feature = "sse",
        feature = "timeout",
        feature = "ws"
    )))
)]
pub use salvo_extra as extra;
