//! Extra features for Salvo web framework.
//!
//! This library provides some common web features.
//!
//! `salvo_extra` uses a set of [feature flags] to reduce the amount of compiled and
//! optional dependencies.
//!
//! # Feature flags
//!
//! | Feature | Description |
//! | --- | --- |
//! | [`affix-state`](affix_state) | Middleware for adding prefix and suffix to the request path |
//! | [`basic-auth`](basic_auth) | Middleware for basic authentication |
//! | [`caching-headers`](caching_headers) | Middleware for setting caching headers |
//! | [`catch-panic`](catch_panic) | Middleware for catching panics |
//! | [`concurrency-limiter`](concurrency_limiter) | Middleware for limiting concurrency |
//! | [`force-https`](force_https) | Middleware for forcing HTTPS |
//! | [`logging`] | Middleware for logging requests and responses |
//! | [`request-id`](request_id) | Middleware for setting a request ID |
//! | [`size-limiter`](size_limiter) | Middleware for limiting request size |
//! | [`sse`] | Server-Sent Events (SSE) middleware |
//! | [`timeout`] | Middleware for setting a timeout |
//! | [`trailing-slash`](trailing_slash) | Middleware for handling trailing slashes |
//! | [`tower-compat`](tower_compat) | Adapters for `tower::Layer` and `tower::Service` |
//! | [`websocket`] | WebSocket implementation |
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "basic-auth"]
    pub mod basic_auth;
}

cfg_feature! {
    #![feature = "affix-state"]
    pub mod affix_state;
}

cfg_feature! {
    #![feature = "force-https"]
    pub mod force_https;
}
cfg_feature! {
    #![feature = "catch-panic"]
    pub mod catch_panic;
}

cfg_feature! {
    #![feature = "logging"]
    pub mod logging;
}
cfg_feature! {
    #![feature = "sse"]
    pub mod sse;
}
cfg_feature! {
    #![feature = "websocket"]
    pub mod websocket;
}
cfg_feature! {
    #![feature = "concurrency-limiter"]
    pub mod concurrency_limiter;
}
cfg_feature! {
    #![feature = "size-limiter"]
    pub mod size_limiter;
}
cfg_feature! {
    #![feature = "trailing-slash"]
    pub mod trailing_slash;
}
cfg_feature! {
    #![feature = "timeout"]
    pub mod timeout;
}
cfg_feature! {
    #![feature = "caching-headers"]
    pub mod caching_headers;
}
cfg_feature! {
    #![feature = "request-id"]
    pub mod request_id;
}
cfg_feature! {
    #![feature ="tower-compat"]
    pub mod tower_compat;
    pub use tower_compat::{TowerServiceCompat, TowerLayerCompat};
}
