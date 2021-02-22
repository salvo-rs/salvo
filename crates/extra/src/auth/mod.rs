pub mod basic;
pub mod jwt;

use base64;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Base64 decode error.")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Parse http header error")]
    ParseHttpHeader,
}
