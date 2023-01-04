//! openssl module
mod config;
pub use config::{Keycert, OpensslConfig};

mod listener;
pub use listener::{OpensslAcceptor, OpensslListener};
