use std::fmt::{self, Formatter, Display};

#[cfg(feature = "http2")]
use crate::runtimes::TokioExecutor;
#[cfg(feature = "http1")]
use hyper::server::conn::http1;
#[cfg(feature = "http2")]
use hyper::server::conn::http2;

#[cfg(feature = "http3")]
use crate::conn::http3;

#[doc(hidden)]
pub struct HttpBuilders {
    #[cfg(feature = "http1")]
    pub(crate) http1: http1::Builder,
    #[cfg(feature = "http2")]
    pub(crate) http2: http2::Builder<TokioExecutor>,
    #[cfg(feature = "http3")]
    pub(crate) http3: http3::Builder,
}

#[doc(hidden)]
#[derive(Copy, Clone, Debug)]
pub enum AppProto {
    Http,
    Https,
    Unknown,
}
impl Display for AppProto {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AppProto::Http => write!(f, "http"),
            AppProto::Https => write!(f, "https"),
            AppProto::Unknown => write!(f, "[unknown]"),
        }
    }
}

#[doc(hidden)]
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum TransProto {
    Udp,
    Tcp,
    Unknown,
}
impl Display for TransProto {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TransProto::Udp => write!(f, "udp"),
            TransProto::Tcp => write!(f, "tcp"),
            TransProto::Unknown => write!(f, "unknown"),
        }
    }
}