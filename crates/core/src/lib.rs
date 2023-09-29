//! The core lib of Savlo web server framework. Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::future_not_send)]
#![warn(rustdoc::broken_intra_doc_links)]

/// Re-export `async_trait`.
pub use async_trait::async_trait;
pub use hyper;
pub use salvo_macros::handler;

pub use salvo_macros as macros;
// https://github.com/bkchr/proc-macro-crate/issues/10
extern crate self as salvo_core;

#[macro_use]
mod cfg;

pub mod catcher;
pub mod conn;
mod depot;
mod error;
pub mod extract;
pub mod fs;
pub mod handler;
pub mod http;
pub mod proto;
pub mod routing;
pub mod rt;
#[doc(hidden)]
pub mod serde;
mod server;
mod service;
pub mod writing;
cfg_feature! {
    #![feature ="test"]
    pub mod test;
}
cfg_feature! {
    #![feature ="quinn"]
    pub use proto::webtransport;
}
cfg_feature! {
    #![feature ="tower-compat"]
    pub mod tower_compat;
    pub use tower_compat::{TowerServiceCompat, TowerLayerCompat};
}

pub use self::conn::Listener;
pub use self::depot::Depot;
pub use self::error::{BoxedError, Error};
pub use self::extract::Extractible;
pub use self::handler::Handler;
pub use self::http::{Request, Response};
pub use self::routing::{FlowCtrl, Router};
pub use self::server::Server;
pub use self::service::Service;
pub use self::writing::{Scribe, Writer};
/// Result type which has `salvo::Error` as it's error type.
pub type Result<T> = std::result::Result<T, Error>;

/// A list of things that automatically imports into application use salvo_core.
pub mod prelude {
    pub use async_trait::async_trait;
    pub use salvo_macros::{handler, Extractible};

    pub use crate::depot::Depot;
    pub use crate::http::{Request, Response, StatusCode, StatusError};
    cfg_feature! {
        #![feature = "acme"]
        pub use crate::conn::AcmeListener;
    }
    cfg_feature! {
        #![feature ="rustls"]
        pub use crate::conn::RustlsListener;
    }
    cfg_feature! {
        #![feature ="native-tls"]
        pub use crate::conn::NativeTlsListener;
    }
    cfg_feature! {
        #![feature ="openssl"]
        pub use crate::conn::OpensslListener;
    }
    cfg_feature! {
        #![feature ="quinn"]
        pub use crate::conn::QuinnListener;
    }
    cfg_feature! {
        #![unix]
        pub use crate::conn::UnixListener;
    }
    cfg_feature! {
        #![feature ="tower-compat"]
        pub use crate::tower_compat::{TowerServiceCompat, TowerLayerCompat};
    }
    pub use crate::conn::{JoinedListener, Listener, TcpListener};
    pub use crate::handler::{self, Handler};
    pub use crate::routing::{FlowCtrl, Router};
    pub use crate::server::Server;
    pub use crate::service::Service;
    pub use crate::writing::{Json, Redirect, Scribe, Text, Writer};
}

#[doc(hidden)]
pub mod __private {
    pub use once_cell;
    pub use tracing;
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
