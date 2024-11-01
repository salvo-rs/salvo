//! OpenTelemetry support for Salvo web framework.
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod metrics;
mod tracing;

pub use metrics::Metrics;
pub use tracing::Tracing;
