//! Extra middleware and compatibility layers for Salvo serverless applications.
//!
//! This crate is the additional compatibility layer for Cloudflare Workers
//! deployments. It mirrors APIs from `salvo_extra` so handlers written for a
//! standard Salvo server can be reused with minimal changes when the runtime
//! is a Worker instead of a TCP listener. Currently it provides a
//! `worker::websocket::WebSocketUpgrade` that follows the same handshake and
//! callback shape as `salvo_extra::websocket::WebSocketUpgrade`.
//!
//! The Workers compatibility layer is compiled only on `wasm32`/`wasm64` targets with
//! the `rt_worker` feature.
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(all(
    feature = "rt_worker",
    any(target_arch = "wasm32", target_arch = "wasm64")
))]
#[cfg_attr(
    docsrs,
    doc(cfg(all(
        feature = "rt_worker",
        any(target_arch = "wasm32", target_arch = "wasm64")
    )))
)]
pub mod worker;
