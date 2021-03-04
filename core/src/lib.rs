mod catcher;
pub mod depot;
mod error;
pub mod fs;
mod handler;
pub mod http;
pub mod routing;
mod server;
mod service;
#[cfg(feature = "tls")]
mod tls;
mod writer;

#[macro_use]
extern crate pin_utils;
#[macro_use]
extern crate futures_util;

pub use self::catcher::{Catcher, CatcherImpl};
pub use self::depot::Depot;
pub use self::error::Error;
pub use self::handler::Handler;
pub use self::http::{Request, Response};
pub use self::routing::Router;
pub use self::server::Server;
pub use self::service::Service;
#[cfg(feature = "tls")]
pub use self::server::TlsServer;
pub use self::writer::Writer;
pub use salvo_macros::fn_handler;
pub type Result<T> = std::result::Result<T, Error>;

pub mod prelude {
    pub use crate::depot::Depot;
    pub use crate::http::errors::*;
    pub use crate::http::{Request, Response, StatusCode};
    pub use crate::routing::filter;
    pub use crate::routing::Router;
    pub use crate::server::Server;
    pub use crate::service::Service;
    #[cfg(feature = "tls")]
    pub use crate::server::TlsServer;
    pub use crate::writer::*;
    pub use crate::Handler;
    pub use async_trait::async_trait;
    pub use salvo_macros::fn_handler;
}
