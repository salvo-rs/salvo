//! The core lib of Savlo web server framework.
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/images/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod addr;
pub mod catcher;
mod depot;
mod error;
pub mod fs;
mod handler;
pub mod http;
pub mod listener;
pub mod routing;
mod server;
mod service;
mod transport;
pub mod writer;

#[cfg(feature = "anyhow")]
pub use anyhow;
pub use hyper;

pub use self::catcher::{Catcher, CatcherImpl};
pub use self::depot::Depot;
pub use self::error::Error;
pub use self::handler::Handler;
pub use self::http::{Request, Response};
#[cfg(feature = "rustls")]
pub use self::listener::RustlsListener;
#[cfg(unix)]
pub use self::listener::UnixListener;
pub use self::listener::{JoinedListener, Listener, TcpListener};
pub use self::routing::Router;
pub use self::server::Server;
pub use self::service::Service;
pub use self::writer::Writer;
pub use async_trait::async_trait;
pub use salvo_macros::fn_handler;
/// Result type wich has salvo::Error as it's error type.
pub type Result<T> = std::result::Result<T, Error>;

/// A list of things that automatically imports into application use salvo.
pub mod prelude {
    pub use crate::depot::Depot;
    pub use crate::http::errors::*;
    pub use crate::http::{Request, Response, StatusCode};
    #[cfg(feature = "rustls")]
    pub use crate::listener::RustlsListener;
    #[cfg(unix)]
    pub use crate::listener::UnixListener;
    pub use crate::listener::{JoinedListener, Listener, TcpListener};
    pub use crate::routing::{filter, Router};
    pub use crate::server::Server;
    pub use crate::service::Service;
    pub use crate::writer::*;
    pub use crate::Handler;
    pub use async_trait::async_trait;
    pub use salvo_macros::fn_handler;
}

use std::future::Future;
use tokio::runtime::{self, Runtime};

fn new_runtime(threads: usize) -> Runtime {
    runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .thread_name("salvo-worker")
        .enable_all()
        .build()
        .unwrap()
}

/// If you don't want to include tokio in your project directly,
/// you can use this function to run server.
/// ```ignore
/// use salvo_core::prelude::*;
/// #[fn_handler]
/// async fn hello_world() -> &'static str {
///     "Hello World"
/// }
/// fn main() {
///
///    let router = Router::new().get(hello_world);
///    let server = Server::new(TcpListener::bind(([0, 0, 0, 0], 7878))).serve(router);
///    salvo_core::run(server);
/// }
/// ```
pub fn run<F: Future>(future: F) {
    run_with_threads(future, num_cpus::get())
}

/// If you don't want to include tokio in your project directly,
/// you can use this function to run server.
/// ```ignore
/// use salvo_core::prelude::*;
/// #[fn_handler]
/// async fn hello_world() -> &'static str {
///     "Hello World"
/// }
/// fn main() {
///    let service = Router::new().get(hello_world);
///    let server = Server::new(TcpListener::bind(([0, 0, 0, 0], 7878))).serve(router).await;
///    salvo_core::run_with_threads(server, 8);
/// }
/// ```
pub fn run_with_threads<F: Future>(future: F, threads: usize) {
    let runtime = crate::new_runtime(threads);
    let _ = runtime.block_on(async { future.await });
}
