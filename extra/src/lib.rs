//! The extra lib of Savlo web server framework.
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/images/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[macro_use]
extern crate serde_derive;
extern crate serde;

#[cfg(feature = "basic-auth")]
#[cfg_attr(docsrs, doc(cfg(feature = "basic-auth")))]
pub mod basic_auth;
#[cfg(feature = "jwt-auth")]
#[cfg_attr(docsrs, doc(cfg(feature = "jwt-auth")))]
pub mod jwt_auth;

#[cfg(feature = "compression")]
#[cfg_attr(docsrs, doc(cfg(feature = "compression")))]
pub mod compression;
#[cfg(feature = "cors")]
#[cfg_attr(docsrs, doc(cfg(feature = "cors")))]
pub mod cors;
#[cfg(feature = "csrf")]
#[cfg_attr(docsrs, doc(cfg(feature = "csrf")))]
pub mod csrf;
#[cfg(feature = "logging")]
#[cfg_attr(docsrs, doc(cfg(feature = "logging")))]
pub mod logging;
#[cfg(feature = "proxy")]
#[cfg_attr(docsrs, doc(cfg(feature = "proxy")))]
pub mod proxy;
#[cfg(feature = "serve-static")]
#[cfg_attr(docsrs, doc(cfg(feature = "serve-static")))]
pub mod serve_static;
#[cfg(feature = "session")]
#[cfg_attr(docsrs, doc(cfg(feature = "session")))]
pub mod session;
#[cfg(feature = "sse")]
#[cfg_attr(docsrs, doc(cfg(feature = "sse")))]
pub mod sse;
#[cfg(feature = "ws")]
#[cfg_attr(docsrs, doc(cfg(feature = "ws")))]
pub mod ws;

#[cfg(feature = "size-limiter")]
#[cfg_attr(docsrs, doc(cfg(feature = "size-limiter")))]
pub mod size_limiter;
#[cfg(feature = "timeout")]
#[cfg_attr(docsrs, doc(cfg(feature = "timeout")))]
pub mod timeout;
