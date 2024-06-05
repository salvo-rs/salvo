//! Utilities for testing application.
//! 
//! # Example
//!
//! ```no_run
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello"
//! }
//!
//! fn route() -> Router {
//!     Router::new().goal(hello)
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
//!     Server::new(acceptor).serve(route()).await;
//! }
//!
//! #[cfg(test)]
//! mod tests {
//!     use salvo_core::prelude::*;
//!     use salvo_core::test::{ResponseExt, TestClient};
//! 
//!     #[tokio::test]
//!     async fn test_hello() {
//!         let service = Service::new(super::route());
//! 
//!         let content = TestClient::get("http://0.0.0.0:5800/")
//!             .send(&service)
//!             .await
//!             .take_string()
//!             .await
//!             .unwrap();
//!         assert!(content.contains("Hello"));
//!     }
//! }
//! ```

mod client;
mod request;
mod response;
pub use client::TestClient;
pub use request::{RequestBuilder, SendTarget};
pub use response::ResponseExt;
