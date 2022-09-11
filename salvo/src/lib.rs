//! Salvo is a powerful and simplest web server framework in Rust world. Read more: <https://salvo.rs>

#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[macro_use]
mod cfg;
pub use salvo_core as core;
#[doc(no_inline)]
pub use salvo_core::*;

cfg_feature! {
    #![any(
        feature = "affix",
        feature = "extra",
        feature = "basic-auth",
        feature = "caching-headers",
        feature = "compression",
        feature = "cors",
        feature = "csrf",
        feature = "force-https",
        feature = "jwt-auth",
        feature = "logging",
        feature = "proxy",
        feature = "serve-static",
        feature = "session",
        feature = "size-limiter",
        feature = "sse",
        feature = "trailing-slash",
        feature = "timeout",
        feature = "ws"
    )]

    #[doc(no_inline)]
    pub use salvo_extra as extra;
}

/// A list of things that automatically imports into application use salvo.
pub mod prelude {
    pub use salvo_core::prelude::*;
    cfg_feature! {
        #![feature ="affix"]
        pub use salvo_extra::affix;
    }
    cfg_feature! {
        #![feature ="basic-auth"]
        pub use salvo_extra::basic_auth::{BasicAuth, BasicAuthDepotExt, BasicAuthValidator};
    }
    cfg_feature! {
        #![feature ="caching-headers"]
        pub use salvo_extra::caching_headers::CachingHeaders;
    }
    cfg_feature! {
        #![feature ="compression"]
        pub use salvo_extra::compression::{Compression, CompressionAlgo};
    }
    cfg_feature! {
        #![feature ="cors"]
        pub use salvo_extra::cors::Cors;
    }
    cfg_feature! {
        #![feature ="csrf"]
        pub use salvo_extra::csrf::{CsrfDepotExt, Csrf};
    }
    cfg_feature! {
        #![feature ="force-https"]
        pub use salvo_extra::force_https::ForceHttps;
    }
    cfg_feature! {
        #![feature ="jwt-auth"]
        pub use salvo_extra::jwt_auth::{JwtAuthDepotExt, JwtAuth};
    }
    cfg_feature! {
        #![feature ="logging"]
        pub use salvo_extra::logging::Logger;
    }
    cfg_feature! {
        #![feature ="proxy"]
        pub use salvo_extra::proxy::Proxy;
    }
    cfg_feature! {
        #![feature ="serve-static"]
        pub use salvo_extra::serve_static::{StaticDir, StaticFile};
    }
    cfg_feature! {
        #![feature ="session"]
        pub use salvo_extra::session::{Session, SessionDepotExt, SessionHandler, SessionStore, MemoryStore, CookieStore};
    }
    cfg_feature! {
        #![feature ="size-limiter"]
        pub use salvo_extra::size_limiter::max_size;
    }
    cfg_feature! {
        #![feature ="sse"]
        pub use salvo_extra::sse::{SseEvent, SseKeepAlive};
    }
    cfg_feature! {
        #![feature ="trailing-slash"]
        pub use salvo_extra::trailing_slash::{self, TrailingSlash, TrailingSlashAction};
    }
    cfg_feature! {
        #![feature ="timeout"]
        pub use salvo_extra::timeout::Timeout;
    }
    cfg_feature! {
        #![feature ="ws"]
        pub use salvo_extra::ws::WebSocketUpgrade;
    }
}
