//! Serverless runtime adapters and helpers for the Salvo web framework.
//!
//! This crate keeps Salvo's request pipeline usable where there is no
//! long-lived TCP listener. A runtime adapter converts the platform event
//! into a Salvo [`Request`](salvo_core::Request), dispatches it through a
//! [`Service`](salvo_core::Service), and converts the response back.
//!
//! # Feature flags
//!
//! | Feature | Description | Default? |
//! | --- | --- | :---: |
//! | `rt_lambda` | AWS Lambda HTTP runtime adapter and `#[service]` entry point | ❌ |
//! | `rt_worker` | Cloudflare Workers runtime adapter and `#[service]` entry point | ❌ |
//!
//! Enable one runtime feature from the `salvo` crate. The `#[service]`
//! attribute emits a `compile_error!` when no runtime feature is enabled.
//!
//! The Workers adapter is compiled only on `wasm32`/`wasm64` targets. Because
//! `#[worker::event]` expands to `::worker::` paths, a downstream crate that
//! uses the generated Workers entry point must declare `worker` as a direct
//! dependency, as is standard for workers-rs projects.
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, allow(clippy::unwrap_used))]

/// Runtime-independent server abstraction.
pub mod server;

/// Runtime logging configuration and initialization.
pub mod log;

/// Commonly used serverless items.
pub mod prelude {
    pub use salvo_serverless_macros::service;

    #[cfg(feature = "rt_lambda")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rt_lambda")))]
    pub use crate::lambda::LambdaServer;
    pub use crate::log::{LogConfig, TracingLog};
    pub use crate::server::GeneralServer;
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
    pub use crate::worker::{WorkerRequestExt, WorkerServer};
}

/// AWS Lambda HTTP runtime adapter.
#[cfg(feature = "rt_lambda")]
#[cfg_attr(docsrs, doc(cfg(feature = "rt_lambda")))]
pub mod lambda;

/// Cloudflare Workers runtime adapter.
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

// Re-exported so `#[service]` can use absolute paths through `salvo::serverless`
// without requiring the downstream crate to depend on these crates directly.
/// Re-export of `lambda_http` used by generated code.
#[cfg(feature = "rt_lambda")]
pub use lambda_http;
pub use salvo_serverless_macros::service;
/// Re-export of `tokio` used by generated code.
#[cfg(feature = "rt_lambda")]
pub use tokio;
