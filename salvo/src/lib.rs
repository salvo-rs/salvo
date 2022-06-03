//! Salvo is a powerful and easy to use web server framework. Read more: <https://salvo.rs>

#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub, unused_crate_dependencies)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[macro_use]
mod cfg;
pub use salvo_core as core;
#[doc(no_inline)]
pub use salvo_core::*;

cfg_feature! {
    #![any(
        feature = "extra",
        feature = "basic-auth",
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
    )]

    #[doc(no_inline)]
    pub use salvo_extra as extra;
}
