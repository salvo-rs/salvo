//! Salvo is a simple but powerful web server framework written in Rust.

pub use salvo_core as core;
pub use salvo_core::*;

#[cfg(any(
    feature = "basic_auth",
    feature = "jwt_auth",
    feature = "compression",
    feature = "proxy",
    feature = "serve",
    feature = "sse",
    feature = "ws",
    feature = "size_limiter"
))]
pub use salvo_extra as extra;
