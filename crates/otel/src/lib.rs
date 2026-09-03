#![cfg_attr(test, allow(clippy::unwrap_used))]
//! OpenTelemetry integration for the Salvo web framework.
//!
//! This crate provides middleware for collecting metrics and distributed traces
//! using the [OpenTelemetry](https://opentelemetry.io/) observability framework.
//!
//! # Components
//!
//! | Middleware | Purpose |
//! |------------|---------|
//! | [`Metrics`] | Collects HTTP server metrics (duration, in-flight requests, body sizes) |
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
//!     .hoop(Tracing::new(provider.tracer("my-service")))
//!     .get(my_handler);
//! ```
//!
//! # Collected Metrics
//!
//! The [`Metrics`] middleware records the HTTP server instruments defined by the
//! [HTTP metric conventions]:
//!
//! - `http.server.request.duration` — request duration histogram, in seconds
//! - `http.server.active_requests` — number of in-flight requests
//! - `http.server.request.body.size` — request body size, in bytes
//! - `http.server.response.body.size` — response body size, in bytes
//!
//! There is no request or error counter: the duration histogram already carries
//! the request count, and failures are selected by filtering on `error.type` or
//! `http.response.status_code`.
//!
//! # Attributes
//!
//! Both middlewares follow the [HTTP span conventions] for attribute names and
//! values: `http.request.method`, `http.route`, `http.response.status_code`,
//! `url.scheme`, `url.path`, `url.query`, `network.protocol.version` and
//! `error.type`.
//!
//! The request target is reported as the matched route template (`/users/{id}`)
//! rather than the requested URI, so the number of metric time series stays
//! proportional to the number of routes. That attribute needs the `matched-path`
//! feature, which is enabled by default.
//!
//! [HTTP metric conventions]: https://opentelemetry.io/docs/specs/semconv/http/http-metrics/
//! [HTTP span conventions]: https://opentelemetry.io/docs/specs/semconv/http/http-spans/
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod metrics;
mod semconv;
mod tracing;

pub use metrics::Metrics;
pub use tracing::Tracing;
