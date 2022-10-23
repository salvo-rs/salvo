use std::sync::Arc;

#[cfg(feature = "http1")]
use hyper::server::conn::http1;
#[cfg(feature = "http2")]
use hyper::server::conn::http2;
#[cfg(feature = "http2")]
use crate::runtimes::TokioExecutor;

#[cfg(feature = "http3")]
use crate::conn::http3::Http3Builder;

pub struct HttpBuilders {
    #[cfg(feature = "http1")]
    pub(crate) http1: http1::Builder,
    #[cfg(feature = "http2")]
    pub(crate) http2: http2::Builder<TokioExecutor>,
    #[cfg(feature = "http3")]
    pub(crate) http3: Http3Builder,
}
