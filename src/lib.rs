//! Salvo is a simple but powerful web server framework written in Rust.

#![doc(html_favicon_url = "https://salvo.rs/images/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
pub use salvo_core as core;
pub use salvo_core::*;

#[cfg(any(
    feature = "basic_auth",
    feature = "compression",
    feature = "cors",
    feature = "csrf",
    feature = "jwt_auth",
    feature = "proxy",
    feature = "serve",
    feature = "size_limiter",
    feature = "sse",
    feature = "ws"
))]
pub use salvo_extra as extra;
