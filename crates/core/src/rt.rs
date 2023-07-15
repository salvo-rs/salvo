//! runtime module.
//!
//! Only supports tokio runtime in current version.
//! More runtimes will be supported in the future.

pub use hyper::rt::*;

/// Tokio runtimes
pub mod tokio {
    pub use salvo_utils::rt::{TokioExecutor, TokioIo};
}

pub use self::tokio::*;
