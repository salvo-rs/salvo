//! TcpListener and it's implements.
use std::io::{Error as IoError, Result as IoResult};
use std::net::SocketAddr;
use std::vec;

use tokio::net::{TcpListener as TokioTcpListener, TcpStream, ToSocketAddrs};

use crate::conn::{Holding, StraightStream};
use crate::fuse::{ArcFuseFactory, TransProto};
use crate::http::uri::Scheme;
use crate::http::Version;

use super::{Accepted, Acceptor, Listener};

#[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl"))]
use crate::conn::IntoConfigStream;

#[cfg(feature = "rustls")]
use crate::conn::rustls::RustlsListener;

#[cfg(feature = "native-tls")]
use crate::conn::native_tls::NativeTlsListener;

#[cfg(feature = "openssl")]
use crate::conn::openssl::OpensslListener;

#[cfg(feature = "acme")]
use crate::conn::acme::AcmeListener;

/// `TcpListener` is used to create a TCP connection listener.
pub struct TcpListener<T> {
    local_addr: T,
}
impl<T: ToSocketAddrs + Send> TcpListener<T> {
    /// Bind to socket address.
    #[inline]
    pub fn new(local_addr: T) -> Self {
        TcpListener { local_addr }
    }

    cfg_feature! {
        #![feature = "rustls"]

        /// Creates a new `RustlsListener` from current `TcpListener`.
        #[inline]
        pub fn rustls<S, C, E>(self, config_stream: S) -> RustlsListener<S, C, Self, E>
        where
            S: IntoConfigStream<C> + Send + 'static,
            C: TryInto<crate::conn::rustls::ServerConfig, Error = E> + Send + 'static,
            E: std::error::Error + Send
        {
            RustlsListener::new(config_stream, self)
        }
    }

    cfg_feature! {
        #![feature = "native-tls"]

        /// Creates a new `NativeTlsListener` from current `TcpListener`.
        #[inline]
        pub fn native_tls<S, C, E>(self, config_stream: S) -> NativeTlsListener<S, C, Self, E>
        where
            S: IntoConfigStream<C> + Send + 'static,
            C: TryInto<crate::conn::native_tls::Identity, Error = E> + Send + 'static,
            E: std::error::Error + Send
        {
            NativeTlsListener::new(config_stream, self)
        }
    }

    cfg_feature! {
        #![feature = "openssl"]

        /// Creates a new `OpensslListener` from current `TcpListener`.
        #[inline]
        pub fn openssl<S, C, E>(self, config_stream: S) -> OpensslListener<S, C, Self, E>
        where
            S: IntoConfigStream<C> + Send + 'static,
            C: TryInto<crate::conn::openssl::SslAcceptorBuilder, Error = E> + Send + 'static,
            E: std::error::Error + Send
        {
            OpensslListener::new(config_stream, self)
        }
    }
    cfg_feature! {
        #![feature = "acme"]

        /// Creates a new `AcmeListener` from current `TcpListener`.
        #[inline]
        pub fn acme(self) -> AcmeListener<Self>
        {
            AcmeListener::new( self)
        }
    }
}
impl<T> Listener for TcpListener<T>
where
    T: ToSocketAddrs + Send,
{
    type Acceptor = TcpAcceptor;

    async fn try_bind(self) -> crate::Result<Self::Acceptor> {
        Ok(TokioTcpListener::bind(self.local_addr).await?.try_into()?)
    }
}
/// `TcpAcceptor` is used to accept a TCP connection.
pub struct TcpAcceptor {
    inner: TokioTcpListener,
    holdings: Vec<Holding>,
}

impl TcpAcceptor {
    /// Returns the local address that this listener is bound to.
    ///
    /// This can be useful, for example, when binding to port 0 to figure out
    /// which port was actually bound.
    pub fn local_addr(&self) -> IoResult<SocketAddr> {
        self.inner.local_addr()
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    pub fn ttl(&self) -> IoResult<u32> {
        self.inner.ttl()
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    pub fn set_ttl(&self, ttl: u32) -> IoResult<()> {
        self.inner.set_ttl(ttl)
    }
}

impl TryFrom<TokioTcpListener> for TcpAcceptor {
    type Error = IoError;
    fn try_from(inner: TokioTcpListener) -> Result<Self, Self::Error> {
        let holding = Holding {
            local_addr: inner.local_addr()?.into(),
            http_versions: vec![Version::HTTP_11],
            http_scheme: Scheme::HTTP,
        };

        Ok(TcpAcceptor {
            inner,
            holdings: vec![holding],
        })
    }
}

impl Acceptor for TcpAcceptor {
    type Conn = StraightStream<TcpStream>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(&mut self, fuse_factory: ArcFuseFactory) -> IoResult<Accepted<Self::Conn>> {
        self.inner.accept().await.map(move |(conn, remote_addr)| Accepted {
            conn: StraightStream::new(conn, fuse_factory.create(TransProto::Tcp)),
            local_addr: self.holdings[0].local_addr.clone(),
            remote_addr: remote_addr.into(),
            http_version: Version::HTTP_11,
            http_scheme: Scheme::HTTP,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;
    use crate::fuse::SteadyFusewire;

    #[tokio::test]
    async fn test_tcp_listener() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 6878));
        let mut acceptor = TcpListener::new(addr).bind().await;
        let addr = acceptor.holdings()[0].local_addr.clone().into_std().unwrap();
        tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            stream.write_i32(150).await.unwrap();
        });

        let Accepted { mut conn, .. } = acceptor.accept(Arc::new(SteadyFusewire)).await.unwrap();
        assert_eq!(conn.read_i32().await.unwrap(), 150);
    }
}
