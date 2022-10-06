//! The core lib of Savlo web server framework. Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use async_trait::async_trait;
pub use hyper;
pub use salvo_macros::handler;

pub use salvo_macros as macros;

#[macro_use]
mod cfg;

pub mod addr;
pub mod catcher;
mod depot;
mod error;
pub mod extract;
pub mod fs;
pub mod handler;
pub mod http;
pub mod listener;
pub mod routing;
pub(crate) mod serde;
mod server;
mod service;
mod transport;
pub mod writer;
cfg_feature! {
    #![feature ="test"]
    pub mod test;
}

pub use self::catcher::{Catcher, CatcherImpl};
pub use self::depot::Depot;
pub use self::error::{BoxedError, Error};
pub use self::extract::Extractible;
pub use self::handler::Handler;
pub use self::http::{Request, Response};
pub use self::listener::Listener;
pub use self::routing::{FlowCtrl, Router};
pub use self::server::Server;
pub use self::service::Service;
pub use self::writer::{Piece, Writer};
/// Result type wich has salvo::Error as it's error type.
pub type Result<T> = std::result::Result<T, Error>;

/// A list of things that automatically imports into application use salvo_core.
pub mod prelude {
    pub use async_trait::async_trait;
    pub use salvo_macros::{handler, Extractible};

    pub use crate::depot::Depot;
    pub use crate::http::{Request, Response, StatusCode, StatusError};
    cfg_feature! {
        #![feature ="acme"]
        pub use crate::listener::AcmeListener;
    }
    cfg_feature! {
        #![feature ="rustls"]
        pub use crate::listener::RustlsListener;
    }
    cfg_feature! {
        #![feature ="native-tls"]
        pub use crate::listener::NativeTlsListener;
    }
    cfg_feature! {
        #![feature ="openssl"]
        pub use crate::listener::OpensslListener;
    }
    cfg_feature! {
        #![unix]
        pub use crate::listener::UnixListener;
    }
    pub use crate::extract::LazyExtract;
    pub use crate::handler::{empty_handler, Handler};
    pub use crate::listener::{JoinedListener, Listener, TcpListener};
    pub use crate::routing::{FlowCtrl, Router};
    pub use crate::server::Server;
    pub use crate::service::Service;
    pub use crate::writer::{Json, Piece, Redirect, Text, Writer};
}

#[doc(hidden)]
pub mod __private {
    pub use once_cell;
    pub use tracing;
}

use std::{future::Future, thread::available_parallelism};

use tokio::runtime::{self, Runtime};

#[inline]
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
///
/// # Example
///
/// ```no_run
/// # use salvo_core::prelude::*;
///
/// #[handler]
/// async fn hello_world() -> &'static str {
///     "Hello World"
/// }
/// #[tokio::main]
/// async fn main() {
///    let router = Router::new().get(hello_world);
///    let server = Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router);
///    salvo_core::run(server);
/// }
/// ```
#[inline]
pub fn run<F: Future>(future: F) {
    run_with_threads(future, available_parallelism().map(|n| n.get()).unwrap_or(1))
}

/// If you don't want to include tokio in your project directly,
/// you can use this function to run server.
///
/// # Example
///
/// ```no_run
/// use salvo_core::prelude::*;
///
/// #[handler]
/// async fn hello_world() -> &'static str {
///     "Hello World"
/// }
///
/// fn main() {
///    let router = Router::new().get(hello_world);
///    let server = Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router);
///    salvo_core::run_with_threads(server, 8);
/// }
/// ```
#[inline]
pub fn run_with_threads<F: Future>(future: F, threads: usize) {
    let runtime = crate::new_runtime(threads);
    let _ = runtime.block_on(async { future.await });
}

#[doc(hidden)]
pub trait IntoVecString {
    fn into_vec_string(self) -> Vec<String>;
}

impl IntoVecString for &'static str {
    fn into_vec_string(self) -> Vec<String> {
        vec![self.to_string()]
    }
}
impl IntoVecString for String {
    fn into_vec_string(self) -> Vec<String> {
        vec![self]
    }
}

impl<const N: usize> IntoVecString for [&'static str; N] {
    fn into_vec_string(self) -> Vec<String> {
        self.into_iter().map(|s| s.into()).collect()
    }
}

impl<T> IntoVecString for Vec<T>
where
    T: Into<String>,
{
    fn into_vec_string(self) -> Vec<String> {
        self.into_iter().map(|s| s.into()).collect()
    }
}

impl<'a, T> IntoVecString for &'a Vec<T>
where
    T: Into<String> + Clone,
{
    fn into_vec_string(self) -> Vec<String> {
        self.iter().map(|s| s.clone().into()).collect()
    }
}
