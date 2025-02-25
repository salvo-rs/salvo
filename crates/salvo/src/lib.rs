//! Salvo is a powerful web framework that can make your work easier.
//!
//! `salvo` uses a set of [feature flags] to reduce the amount of compiled and
//! optional dependencies.
//!
//! # Feature flags
//!
//! | Feature | Description | Default? |
//! | --- | --- | :---: |
//! | `cookie` | Support for Cookie | ✔️ |
//! | `server` | Built-in Server implementation | ✔️ |
//! | `http1` | Support for HTTP 1.1 protocol | ✔️ |
//! | `http2` | Support for HTTP 2 protocol | ✔️ |
//! | `http2-cleartext` | Support for HTTP 2 over cleartext TCP | ❌ |
//! | `quinn` | Use [quinn](https://crates.io/crates/quinn) to support HTTP 3 protocol | ❌ |
//! | `test` | Utilities for testing application | ✔️ |
//! | `acme` | Automatically obtain certificates through ACME | ❌ |
//! | `rustls` | TLS built on [`rustls`](https://crates.io/crates/rustls) | ❌ |
//! | `openssl` | TLS built on [`openssl-tls`](https://crates.io/crates/openssl) | ❌ |
//! | `native-tls` | TLS built on [`native-tls`](https://crates.io/crates/native-tls) | ❌ |
//! | `unix` | Listener based on Unix socket | ❌ |
//! | `tower-compat` | Adapters for `tower::Layer` and `tower::Service` | ❌ |
//! | `anyhow` | Integrate with the [`anyhow`](https://crates.io/crates/anyhow) crate | ❌ |
//! | `eyre` | Integrate with the [`eyre`](https://crates.io/crates/eyre) crate | ❌ |
//! | `affix-state` | Middleware for adding prefix and suffix to the request path | ❌ |
//! | `craft` | Generate handlers or endpoints with shared data | ❌ |
//! | `basic-auth` | Middleware for basic authentication | ❌ |
//! | `caching-headers` | Middleware for setting caching headers | ❌ |
//! | `catch-panic` | Middleware for catching panics | ❌ |
//! | `concurrency-limiter` | Middleware for limiting concurrency | ❌ |
//! | `force-https` | Middleware for forcing HTTPS | ❌ |
//! | `logging` | Middleware for logging requests and responses | ❌ |
//! | `request-id` | Middleware for setting a request ID | ❌ |
//! | `size-limiter` | Middleware for limiting request size | ❌ |
//! | `sse` | Server-Sent Events (SSE) middleware | ❌ |
//! | `timeout` | Middleware for setting a timeout | ❌ |
//! | `trailing-slash` | Middleware for handling trailing slashes | ❌ |
//! | `websocket` | WebSocket implementation | ❌ |
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
mod cfg;
pub use salvo_core as core;
// #[doc(no_inline)]
pub use salvo_core::*;
// https://github.com/bkchr/proc-macro-crate/issues/10
extern crate self as salvo;

cfg_feature! {
    #![feature ="affix-state"]
    // #[doc(no_inline)]
    pub use salvo_extra::affix_state;
}
cfg_feature! {
    #![feature ="basic-auth"]
    // #[doc(no_inline)]
    pub use salvo_extra::basic_auth;
}
cfg_feature! {
    #![feature ="caching-headers"]
    // #[doc(no_inline)]
    pub use salvo_extra::caching_headers;
}
cfg_feature! {
    #![feature ="catch-panic"]
    // #[doc(no_inline)]
    pub use salvo_extra::catch_panic;
}
cfg_feature! {
    #![feature ="force-https"]
    // #[doc(no_inline)]
    pub use salvo_extra::force_https;
}
cfg_feature! {
    #![feature ="logging"]
    // #[doc(no_inline)]
    pub use salvo_extra::logging;
}
cfg_feature! {
    #![feature ="concurrency-limiter"]
    // #[doc(no_inline)]
    pub use salvo_extra::concurrency_limiter;
}
cfg_feature! {
    #![feature ="size-limiter"]
    // #[doc(no_inline)]
    pub use salvo_extra::size_limiter;
}
cfg_feature! {
    #![feature ="sse"]
    // #[doc(no_inline)]
    pub use salvo_extra::sse;
}
cfg_feature! {
    #![feature ="trailing-slash"]
    // #[doc(no_inline)]
    pub use salvo_extra::trailing_slash;
}
cfg_feature! {
    #![feature ="timeout"]
    // #[doc(no_inline)]
    pub use salvo_extra::timeout;
}
cfg_feature! {
    #![feature ="websocket"]
    // #[doc(no_inline)]
    pub use salvo_extra::websocket;
}
cfg_feature! {
    #![feature ="request-id"]
    // #[doc(no_inline)]
    pub use salvo_extra::request_id;
}
cfg_feature! {
    #![feature ="cache"]
    #[doc(no_inline)]
    pub use salvo_cache as cache;
}
cfg_feature! {
    #![feature ="compression"]
    #[doc(no_inline)]
    pub use salvo_compression as compression;
}
cfg_feature! {
    #![feature ="cors"]
    #[doc(no_inline)]
    pub use salvo_cors as cors;
}
cfg_feature! {
    #![feature ="craft"]
    // #[doc(no_inline)]
    pub use salvo_craft as craft;
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
    #![feature ="jwt-auth"]
    #[doc(no_inline)]
    pub use salvo_jwt_auth as jwt_auth;
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
cfg_feature! {
    #![feature ="otel"]
    #[doc(no_inline)]
    pub use salvo_otel as otel;
}
cfg_feature! {
    #![feature ="oapi"]
    #[doc(no_inline)]
    pub use salvo_oapi as oapi;
}

/// A list of things that automatically imports into application use salvo.
pub mod prelude {
    pub use salvo_core::prelude::*;
    cfg_feature! {
        #![feature ="affix-state"]
        pub use salvo_extra::affix_state;
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
        pub use salvo_compression::{Compression, CompressionAlgo, CompressionLevel};
    }
    cfg_feature! {
        #![feature ="craft"]
        // #[doc(no_inline)]
        pub use salvo_craft::craft;
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
        pub use salvo_jwt_auth::{JwtAuthDepotExt, JwtAuth, JwtAuthState};
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
        #![feature ="session"]
        pub use salvo_session::{SessionDepotExt, SessionHandler, SessionStore};
    }
    cfg_feature! {
        #![feature ="concurrency-limiter"]
        pub use salvo_extra::concurrency_limiter::max_concurrency;
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
        #![feature ="tower-compat"]
        pub use salvo_extra::tower_compat::{TowerServiceCompat, TowerLayerCompat};
    }
    cfg_feature! {
        #![feature ="websocket"]
        pub use salvo_extra::websocket::WebSocketUpgrade;
    }
    cfg_feature! {
        #![feature ="request-id"]
        pub use salvo_extra::request_id::RequestId;
    }
    cfg_feature! {
        #![feature ="serve-static"]
        pub use salvo_serve_static::{StaticFile, StaticDir};
    }
    cfg_feature! {
        #![feature ="oapi"]
        pub use crate::oapi::{endpoint, RouterExt, EndpointArgRegister, EndpointOutRegister, OpenApi, ToParameter, ToParameters, ToSchema, ToResponse, ToResponses};
        pub use crate::oapi::swagger_ui::SwaggerUi;
        pub use crate::oapi::rapidoc::RapiDoc;
        pub use crate::oapi::redoc::ReDoc;
        pub use crate::oapi::scalar::Scalar;
    }
}
