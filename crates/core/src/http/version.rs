use std::future::Future;
use std::io::Result as IoResult;
use std::sync::Arc;

use futures_util::stream::{Peekable, Stream, StreamExt};

pub use http::version::Version;

use crate::async_trait;
use crate::conn::HttpBuilders;
use crate::service::HyperHandler;

/// A helper trait for get a protocol from certain types.
#[async_trait]
pub trait HttpConnection {
    async fn http_version(&mut self) -> Option<Version>;
    async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()>;
}

pub(crate) fn from_alpn(proto: impl AsRef<[u8]>) -> Version {
    if proto.as_ref().windows(2).any(|window| window == b"h2") {
        Version::HTTP_2
    } else {
        Version::HTTP_11
    }
}
