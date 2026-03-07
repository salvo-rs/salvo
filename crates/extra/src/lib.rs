//! Extra middleware and utilities for the Salvo web framework.
//!
//! This crate provides a collection of commonly-needed web features as middleware
//! and utilities that complement Salvo's core functionality. All features are
//! opt-in through feature flags, allowing you to include only what you need.
//!
//! # Quick Start
//!
//! Add `salvo_extra` to your `Cargo.toml` with the features you need:
//!
//! ```toml
//! [dependencies]
//! salvo_extra = { version = "0.88", features = ["basic-auth", "websocket"] }
//! ```
//!
//! Or use the `salvo` umbrella crate which re-exports these features:
//!
//! ```toml
//! [dependencies]
//! salvo = { version = "0.88", features = ["basic-auth", "websocket"] }
//! ```
//!
//! # Feature Categories
//!
//! ## Authentication & Security
//!
//! | Feature | Description |
//! |---------|-------------|
//! | [`basic-auth`](basic_auth) | HTTP Basic Authentication (RFC 7617) |
//! | [`force-https`](force_https) | Redirect HTTP requests to HTTPS |
//!
//! ## Request/Response Processing
//!
//! | Feature | Description |
//! |---------|-------------|
//! | [`caching-headers`](caching_headers) | Add cache control headers |
//! | [`size-limiter`](size_limiter) | Limit request body size |
//! | [`timeout`] | Set request processing timeout |
//! | [`trailing-slash`](trailing_slash) | Normalize URL trailing slashes |
//!
//! ## Concurrency & Resource Management
//!
//! | Feature | Description |
//! |---------|-------------|
//! | [`concurrency-limiter`](concurrency_limiter) | Limit concurrent requests |
//! | [`affix-state`](affix_state) | Attach shared state to requests |
//!
//! ## Observability
//!
//! | Feature | Description |
//! |---------|-------------|
//! | [`logging`] | Request/response logging |
//! | [`request-id`](request_id) | Unique request ID generation |
//!
//! ## Real-time Communication
//!
//! | Feature | Description |
//! |---------|-------------|
//! | [`sse`] | Server-Sent Events for streaming updates |
//! | [`websocket`] | Full-duplex WebSocket connections |
//!
//! ## Error Handling & Integration
//!
//! | Feature | Description |
//! |---------|-------------|
//! | [`catch-panic`](catch_panic) | Convert panics to error responses |
//! | [`tower-compat`](tower_compat) | Use Tower middleware with Salvo |
//!
//! # Usage Example
//!
//! ```ignore
//! use salvo::prelude::*;
//! use salvo_extra::basic_auth::{BasicAuth, BasicAuthValidator};
//! use salvo_extra::logging::Logger;
//! use salvo_extra::timeout::Timeout;
//! use std::time::Duration;
//!
//! // Apply multiple middleware
//! let router = Router::new()
//!     .hoop(Logger::new())
//!     .hoop(Timeout::new(Duration::from_secs(30)))
//!     .push(
//!         Router::with_path("api")
//!             .hoop(BasicAuth::new(MyValidator))
//!             .get(my_handler)
//!     );
//! ```
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
