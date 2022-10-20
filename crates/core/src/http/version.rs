use std::future::Future;

use futures_util::stream::{Peekable, StreamExt, Stream};

pub use http::version::Version;

use crate::async_trait;

/// A helper trait for get a protocol from certain types.
#[async_trait]
pub trait VersionDetector {
    async fn http_version(&mut self) -> Option<Version>;
}

pub(crate) fn from_alpn(proto: impl AsRef<[u8]>) -> Version {
    if proto.as_ref().windows(2).any(|window| window == b"h2") {
        Version::HTTP_2
    } else {
        Version::HTTP_11
    }
}