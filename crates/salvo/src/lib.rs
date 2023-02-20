//! Salvo is a powerful and simple Rust web server framework. Read more: <https://salvo.rs>

#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::future_not_send)]

#[macro_use]
mod cfg;
pub use salvo_core as core;
#[doc(no_inline)]
pub use salvo_core::*;

cfg_feature! {
    #![feature ="affix"]
    #[doc(no_inline)]
    pub use salvo_extra::affix;
}
cfg_feature! {
    #![feature ="basic-auth"]
    #[doc(no_inline)]
    pub use salvo_extra::basic_auth;
}
cfg_feature! {
    #![feature ="caching-headers"]
    #[doc(no_inline)]
    pub use salvo_extra::caching_headers;
}
cfg_feature! {
    #![feature ="catch-panic"]
    #[doc(no_inline)]
    pub use salvo_extra::catch_panic;
}
cfg_feature! {
    #![feature ="compression"]
    #[doc(no_inline)]
    pub use salvo_extra::compression;
}
cfg_feature! {
    #![feature ="force-https"]
    #[doc(no_inline)]
    pub use salvo_extra::force_https;
}
cfg_feature! {
    #![feature ="jwt-auth"]
    #[doc(no_inline)]
    pub use salvo_extra::jwt_auth;
}
cfg_feature! {
    #![feature ="logging"]
    #[doc(no_inline)]
    pub use salvo_extra::logging;
}
cfg_feature! {
    #![feature ="size-limiter"]
    #[doc(no_inline)]
    pub use salvo_extra::size_limiter;
}
cfg_feature! {
    #![feature ="sse"]
    #[doc(no_inline)]
    pub use salvo_extra::sse;
}
cfg_feature! {
    #![feature ="trailing-slash"]
    #[doc(no_inline)]
    pub use salvo_extra::trailing_slash;
}
cfg_feature! {
    #![feature ="timeout"]
    #[doc(no_inline)]
    pub use salvo_extra::timeout;
}
cfg_feature! {
    #![feature ="ws"]
    #[doc(no_inline)]
    pub use salvo_extra::ws;
}
cfg_feature! {
    #![feature ="cache"]
    #[doc(no_inline)]
    pub use salvo_cache as cache;
}
cfg_feature! {
    #![feature ="cors"]
    #[doc(no_inline)]
    pub use salvo_cors as cors;
}
cfg_feature! {
    #![feature ="csrf"]
    #[doc(no_inline)]
    pub use salvo_csrf as csrf;
}
cfg_feature! {
    #![feature ="flash"]
    #[doc(no_inline)]
    pub use salvo_flash as flash;
}
cfg_feature! {
    #![feature ="proxy"]
    #[doc(no_inline)]
    pub use salvo_proxy as proxy;
}
cfg_feature! {
    #![feature ="rate-limiter"]
    #[doc(no_inline)]
    pub use salvo_rate_limiter as rate_limiter;
}
cfg_feature! {
    #![feature ="session"]
    #[doc(no_inline)]
    pub use salvo_session as session;
}
cfg_feature! {
    #![feature ="serve-static"]
    #[doc(no_inline)]
    pub use salvo_serve_static as serve_static;
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
        #![feature ="catch-panic"]
        pub use salvo_extra::catch_panic::CatchPanic;
    }
    cfg_feature! {
        #![feature ="compression"]
        pub use salvo_extra::compression::{Compression, CompressionAlgo};
    }
    cfg_feature! {
        #![feature ="csrf"]
        pub use salvo_csrf::CsrfDepotExt;
    }
    cfg_feature! {
        #![feature ="force-https"]
        pub use salvo_extra::force_https::ForceHttps;
    }
    cfg_feature! {
        #![feature ="jwt-auth"]
        pub use salvo_extra::jwt_auth::{JwtAuthDepotExt, JwtAuth, JwtAuthState};
    }
    cfg_feature! {
        #![feature ="logging"]
        pub use salvo_extra::logging::Logger;
    }
    cfg_feature! {
        #![feature ="proxy"]
        pub use salvo_proxy::Proxy;
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
    cfg_feature! {
        #![feature ="serve-static"]
        pub use salvo_serve_static::{StaticFile, StaticDir};
    }
}
