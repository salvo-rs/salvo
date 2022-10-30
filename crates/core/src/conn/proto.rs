#[cfg(feature = "http2")]
use crate::runtimes::TokioExecutor;
#[cfg(feature = "http1")]
use hyper::server::conn::http1;
#[cfg(feature = "http2")]
use hyper::server::conn::http2;

#[cfg(feature = "quinn")]
use crate::conn::quinn;

#[doc(hidden)]
pub struct HttpBuilders {
    #[cfg(feature = "http1")]
    pub(crate) http1: http1::Builder,
    #[cfg(feature = "http2")]
    pub(crate) http2: http2::Builder<TokioExecutor>,
    #[cfg(feature = "quinn")]
    pub(crate) quinn: quinn::Builder,
}
