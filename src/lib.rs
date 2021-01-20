pub use salvo_core as core;
pub use salvo_core::*;

#[cfg(feature = "macros")]
pub use salvo_macros;

#[cfg(feature = "extra")]
pub use salvo_extra as extra;

pub mod prelude {
    pub use crate::depot::Depot;
    pub use crate::http::{Request, Response, StatusCode, HttpError};
    pub use crate::routing::filter;
    pub use crate::routing::Router;
    pub use crate::server::Server;
    pub use crate::writer::*;
    pub use crate::Handler;
    pub use async_trait::async_trait;
    #[cfg(feature = "macros")]
    pub use salvo_macros::fn_handler;
}
