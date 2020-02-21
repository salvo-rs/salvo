pub use salvo_core as core;
pub use salvo_core::*;

// pub use salvo_macros;

#[cfg(feature = "extra")]
pub use salvo_extra as extra;

pub mod prelude {
    pub use crate::server::{Server, ServerConfig};
    pub use crate::routing::Router;
    pub use crate::depot::Depot;
    pub use crate::http::{Request, Response};
    pub use crate::Handler;
    pub use crate::logging::{self, logger};
    pub use std::sync::Arc;
    // pub use salvo_macros::fn_handler;
    pub use async_trait::async_trait;
}