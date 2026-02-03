//! OpenTelemetry integration for the Salvo web framework.
//!
//! This crate provides middleware for collecting metrics and distributed traces
//! using the [OpenTelemetry](https://opentelemetry.io/) observability framework.
//!
//! # Components
//!
//! | Middleware | Purpose |
//! |------------|---------|
//! | [`Metrics`] | Collects HTTP request metrics (latency, status codes, etc.) |
//! | [`Tracing`] | Adds distributed tracing spans to requests |
//!
//! # Metrics Example
//!
//! ```ignore
//! use salvo_otel::Metrics;
//! use salvo_core::prelude::*;
//! use opentelemetry::global;
//! use opentelemetry_sdk::metrics::SdkMeterProvider;
//!
//! // Initialize OpenTelemetry metrics provider
//! let provider = SdkMeterProvider::builder().build();
//! global::set_meter_provider(provider);
//!
//! let router = Router::new()
//!     .hoop(Metrics::new())
//!     .get(my_handler);
//! ```
//!
//! # Tracing Example
//!
//! ```ignore
//! use salvo_otel::Tracing;
//! use salvo_core::prelude::*;
//! use opentelemetry::global;
//! use opentelemetry_sdk::trace::TracerProvider;
//!
//! // Initialize OpenTelemetry tracing provider
//! let provider = TracerProvider::builder().build();
//! global::set_tracer_provider(provider);
//!
//! let router = Router::new()
//!     .hoop(Tracing::new())
//!     .get(my_handler);
//! ```
//!
//! # Collected Metrics
//!
//! The `Metrics` middleware collects:
//! - `http.server.request.duration` - Request duration histogram
//! - `http.server.active_requests` - Number of in-flight requests
//! - `http.server.request.body.size` - Request body size
//! - `http.server.response.body.size` - Response body size
//!
//! # Trace Attributes
//!
//! The `Tracing` middleware adds standard HTTP semantic conventions:
//! - `http.method` - HTTP method
//! - `http.route` - Matched route pattern
//! - `http.status_code` - Response status code
//! - `http.url` - Request URL
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod metrics;
mod tracing;

pub use metrics::Metrics;
pub use tracing::Tracing;
