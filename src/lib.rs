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
    feature = "basic_auth",
    feature = "compression",
    feature = "cors",
    feature = "csrf",
    feature = "jwt-auth",
    feature = "proxy",
    feature = "serve",
    feature = "session",
    feature = "size_limiter",
    feature = "sse",
    feature = "ws"
))]
pub use salvo_extra as extra;
