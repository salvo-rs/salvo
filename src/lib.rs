pub use salvo_core as core;
pub use salvo_core::*;

#[cfg(feature = "basic_auth")]
#[cfg(feature = "jwt_auth")]
#[cfg(feature = "compression")]
#[cfg(feature = "proxy")]
#[cfg(feature = "serve")]
#[cfg(feature = "sse")]
#[cfg(feature = "ws")]
#[cfg(feature = "size_limiter")]
pub use salvo_extra as extra;
