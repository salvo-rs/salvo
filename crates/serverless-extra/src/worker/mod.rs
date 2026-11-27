//! Cloudflare Workers compatibility layers built on top of `salvo-serverless`.
//!
//! These modules are only compiled for `wasm32`/`wasm64` targets when the `rt_worker`
//! feature is enabled.

pub mod websocket;
