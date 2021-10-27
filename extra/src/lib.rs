#[macro_use]
extern crate serde_derive;
extern crate serde;

#[cfg(feature = "basic_auth")]
pub mod basic_auth;
#[cfg(feature = "jwt_auth")]
pub mod jwt_auth;

#[cfg(feature = "compression")]
pub mod compression;
#[cfg(feature = "cors")]
pub mod cors;
#[cfg(feature = "csrf")]
pub mod csrf;
#[cfg(feature = "proxy")]
pub mod proxy;
#[cfg(feature = "serve")]
pub mod serve;
#[cfg(feature = "sse")]
pub mod sse;
#[cfg(feature = "ws")]
pub mod ws;

#[cfg(feature = "size_limiter")]
pub mod size_limiter;
