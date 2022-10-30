//! HTTP/3 client and server
#![deny(missing_docs)]

pub mod error;
pub mod quic;
pub mod server;

pub use error::Error;

mod buf;
pub mod client;
mod connection;
mod frame;
mod proto;
#[allow(dead_code)]
mod qpack;
mod stream;

pub mod quinn_impl;

#[cfg(test)]
mod tests;

pub use quinn::{
    self, crypto::Session, Endpoint, IncomingBiStreams, IncomingUniStreams, NewConnection, OpenBi, OpenUni, VarInt,
    WriteError,
};
