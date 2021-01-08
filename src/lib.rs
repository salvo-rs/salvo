pub use salvo_core as core;
pub use salvo_core::*;

#[cfg(feature = "macro")]
pub use salvo_macro;

#[cfg(feature = "extra")]
pub use salvo_extra as extra;

pub mod prelude {
    pub use crate::depot::Depot;
    pub use crate::http::{Request, Response};
    pub use crate::routing::Router;
    pub use crate::routing::filter;
    pub use crate::server::{Server, ServerConfig};
    pub use crate::writer::*;
    pub use crate::Handler;
    pub use async_trait::async_trait;
    #[cfg(feature = "macro")]
    pub use salvo_macro::fn_handler;
    pub use std::sync::Arc;
}
