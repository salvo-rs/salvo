use hyper::server::conn::AddrStream;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::addr::SocketAddr;

pub trait Transport: AsyncRead + AsyncWrite {
    fn remote_addr(&self) -> Option<SocketAddr>;
}

impl Transport for AddrStream {
    #[inline]
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr().into())
    }
}
