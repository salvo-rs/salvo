//! runtime module.
//!
//! Only supports tokio runtime in current version.
//! More runtimes will be supported in the future.
pub mod tokio;

pub use self::tokio::{TokioExecutor, TokioTimer};
