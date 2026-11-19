//! Cloudflare Workers runtime adapter.
//!
//! This module is compiled only for `wasm32`/`wasm64` targets when the
//! `rt_worker` feature is enabled. It converts between [`worker::HttpRequest`] and Salvo's
//! request/response types and exposes [`WorkerServer`] used by the `#[service]`
//! generated entry point.
//!
//! The `#[worker::event]` macro expands to hard-coded `::worker::` paths, so a
//! downstream crate must declare `worker` as a direct dependency (the standard
//! workers-rs usage). Salvo re-exports the types required by the generated code
//! from this module, but those macro-internal paths cannot be redirected.

mod bindings;
mod request;
mod response;
mod server;

// Re-exports used by the `#[service]` generated entry point. The attribute
// macro is referenced as `worker::event` there, so the macro itself and the
// `Result` alias must be resolvable through this module as well.
pub use ::worker::{Context, Env, HttpRequest, HttpResponse, Result, event};
pub use bindings::WorkerRequestExt;
pub use server::WorkerServer;

/// Carries a Workers websocket client handle from a Salvo response into the
/// Workers response conversion layer.
///
/// Handlers that upgrade a request to a websocket insert this handle into
/// [`Response::extensions`](salvo_core::Response::extensions). The response
/// converter then builds a `worker::Response` from it instead of buffering the
/// response body into bytes.
#[derive(Debug, Clone)]
pub struct WorkerWebSocketHandle(pub worker::WebSocket);
