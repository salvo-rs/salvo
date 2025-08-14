//! TcpListener and it's implements.
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, Result as IoResult};
use std::net::SocketAddr;
use std::sync::Arc;
use std::vec;

use futures_util::future::{BoxFuture, FutureExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener as TokioTcpListener, TcpStream, ToSocketAddrs};
use tokio_util::sync::CancellationToken;

use crate::conn::{Holding, HttpBuilder, StraightStream};
use crate::fuse::{ArcFuseFactory, FuseEvent, FuseInfo, TransProto};
use crate::http::Version;
use crate::http::uri::Scheme;
use crate::service::HyperHandler;

use super::{Accepted, Acceptor, Coupler, DynStream, Listener};

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
    ttl: Option<u32>,
    #[cfg(feature = "socket2")]
    backlog: Option<u32>,
}
impl<T: Debug> Debug for TcpListener<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpListener")
            .field("local_addr", &self.local_addr)
            .field("ttl", &self.ttl)
            .finish()
    }
}
impl<T: ToSocketAddrs + Send + 'static> TcpListener<T> {
    /// Bind to socket address.
    #[cfg(not(feature = "socket2"))]
    #[inline]
    pub fn new(local_addr: T) -> Self {
        #[cfg(not(feature = "socket2"))]
        Self {
            local_addr,
            ttl: None,
        }
    }
    /// Bind to socket address.
    #[cfg(feature = "socket2")]
    #[inline]
    pub fn new(local_addr: T) -> Self {
        TcpListener {
            local_addr,
            ttl: None,
            backlog: None,
        }
    }

    cfg_feature! {
        #![feature = "rustls"]

        /// Creates a new `RustlsListener` from current `TcpListener`.
        #[inline]
        pub fn rustls<S, C, E>(self, config_stream: S) -> RustlsListener<S, C, Self, E>
        where
            S: IntoConfigStream<C> + Send + 'static,
            C: TryInto<crate::conn::rustls::ServerConfig, Error = E> + Send + 'static,
            E: std::error::Error + Send + 'static
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
            E: std::error::Error + Send + 'static
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
            E: std::error::Error + Send + 'static
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
            AcmeListener::new(self)
        }
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    #[must_use]
    pub fn ttl(mut self, ttl: u32) -> Self {
        self.ttl = Some(ttl);
        self
    }

    cfg_feature! {
        #![feature = "socket2"]
        /// Set backlog capacity.
        #[inline]
        pub fn backlog(mut self, backlog: u32) -> Self {
            self.backlog = Some(backlog);
            self
        }
    }
}
impl<T> Listener for TcpListener<T>
where
    T: ToSocketAddrs + Send + 'static,
{
    type Acceptor = TcpAcceptor;

    async fn try_bind(self) -> crate::Result<Self::Acceptor> {
        let inner = TokioTcpListener::bind(self.local_addr).await?;

        #[cfg(feature = "socket2")]
        if let Some(backlog) = self.backlog {
            let socket = socket2::SockRef::from(&inner);
            socket.listen(backlog as _)?;
        }
        if let Some(ttl) = self.ttl {
            inner.set_ttl(ttl)?;
        }

        Ok(inner.try_into()?)
    }
}
/// `TcpAcceptor` is used to accept a TCP connection.
#[derive(Debug)]
pub struct TcpAcceptor {
    inner: TokioTcpListener,
    holdings: Vec<Holding>,
}

impl TcpAcceptor {
    /// Get the inner `TokioTcpListener`.
    pub fn inner(&self) -> &TokioTcpListener {
        &self.inner
    }

    /// Get the local address that this listener is bound to.
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

    /// Convert this `TcpAcceptor` into a boxed `DynTcpAcceptor`.
    pub fn into_boxed(self) -> Box<dyn DynTcpAcceptor> {
        Box::new(ToDynTcpAcceptor(self))
    }
}

impl TryFrom<TokioTcpListener> for TcpAcceptor {
    type Error = IoError;
    fn try_from(inner: TokioTcpListener) -> Result<Self, Self::Error> {
        let holdings = vec![Holding {
            local_addr: inner.local_addr()?.into(),
            #[cfg(not(feature = "http2-cleartext"))]
            http_versions: vec![Version::HTTP_11],
            #[cfg(feature = "http2-cleartext")]
            http_versions: vec![Version::HTTP_11, Version::HTTP_2],
            http_scheme: Scheme::HTTP,
        }];

        Ok(Self { inner, holdings })
    }
}

impl Acceptor for TcpAcceptor {
    type Coupler = TcpCoupler<Self::Stream>;
    type Stream = StraightStream<TcpStream>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        self.inner.accept().await.map(move |(conn, remote_addr)| {
            let local_addr = self.holdings[0].local_addr.clone();
            let fusewire = fuse_factory.map(|f| {
                f.create(FuseInfo {
                    trans_proto: TransProto::Tcp,
                    remote_addr: remote_addr.into(),
                    local_addr: local_addr.clone(),
                })
            });
            Accepted {
                coupler: TcpCoupler::new(),
                stream: StraightStream::new(conn, fusewire.clone()),
                fusewire,
                remote_addr: remote_addr.into(),
                local_addr,
                http_scheme: Scheme::HTTP,
            }
        })
    }
}

#[doc(hidden)]
pub struct TcpCoupler<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    _marker: std::marker::PhantomData<S>,
}
impl<S> TcpCoupler<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    /// Create a new `TcpCoupler`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}
impl<S> Default for TcpCoupler<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Coupler for TcpCoupler<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Stream = S;

    fn couple(
        &self,
        stream: Self::Stream,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        graceful_stop_token: Option<CancellationToken>,
    ) -> BoxFuture<'static, IoResult<()>> {
        let fusewire = handler.fusewire.clone();
        if let Some(fusewire) = &fusewire {
            fusewire.event(FuseEvent::Alive);
        }
        async move {
            builder
                .serve_connection(stream, handler, fusewire, graceful_stop_token)
                .await
                .map_err(IoError::other)
        }
        .boxed()
    }
}
impl<S> Debug for TcpCoupler<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpCoupler").finish()
    }
}

/// Dynamic TCP acceptor trait.
pub trait DynTcpAcceptor: Send {
    /// Returns the holdings of the acceptor.
    fn holdings(&self) -> &[Holding];

    /// Accept a new connection.
    fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> BoxFuture<'_, IoResult<Accepted<TcpCoupler<DynStream>, DynStream>>>;
}
impl Acceptor for dyn DynTcpAcceptor {
    type Coupler = TcpCoupler<DynStream>;
    type Stream = DynStream;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        DynTcpAcceptor::holdings(self)
    }

    #[inline]
    async fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        DynTcpAcceptor::accept(self, fuse_factory).await
    }
}

/// Convert an `Acceptor` into a boxed `DynTcpAcceptor`.
pub struct ToDynTcpAcceptor<A>(pub A);
impl<A: Acceptor + 'static> DynTcpAcceptor for ToDynTcpAcceptor<A> {
    #[inline]
    fn holdings(&self) -> &[Holding] {
        self.0.holdings()
    }

    #[inline]
    fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> BoxFuture<'_, IoResult<Accepted<TcpCoupler<DynStream>, DynStream>>> {
        async move {
            let accepted = self.0.accept(fuse_factory).await?;
            Ok(accepted.map_into(|_| TcpCoupler::new(), DynStream::new))
        }
        .boxed()
    }
}
impl<A: Debug> Debug for ToDynTcpAcceptor<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToDynTcpAcceptor")
            .field("inner", &self.0)
            .finish()
    }
}

/// Dynamic TCP acceptors.
pub struct DynTcpAcceptors {
    inners: Vec<Box<dyn DynTcpAcceptor>>,
    holdings: Vec<Holding>,
}
impl DynTcpAcceptors {
    /// Create a new `DynTcpAcceptors`.
    #[must_use]
    pub fn new(inners: Vec<Box<dyn DynTcpAcceptor>>) -> Self {
        let holdings = inners
            .iter()
            .flat_map(|inner| inner.holdings())
            .cloned()
            .collect();
        Self { inners, holdings }
    }
}
impl DynTcpAcceptor for DynTcpAcceptors {
    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> BoxFuture<'_, IoResult<Accepted<TcpCoupler<DynStream>, DynStream>>> {
        async move {
            let mut set = Vec::new();
            for inner in &mut self.inners {
                let fuse_factory = fuse_factory.clone();
                set.push(async move { inner.accept(fuse_factory).await }.boxed());
            }
            futures_util::future::select_all(set.into_iter()).await.0
        }
        .boxed()
    }
}
impl Acceptor for DynTcpAcceptors {
    type Coupler = TcpCoupler<DynStream>;
    type Stream = DynStream;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        DynTcpAcceptor::holdings(self)
    }

    #[inline]
    async fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        DynTcpAcceptor::accept(self, fuse_factory).await
    }
}
impl Debug for DynTcpAcceptors {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynTcpAcceptors")
            .field("holdings", &self.holdings)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn test_tcp_listener() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 6878));
        let mut acceptor = TcpListener::new(addr).bind().await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            stream.write_i32(150).await.unwrap();
        });

        let Accepted { mut stream, .. } = acceptor.accept(None).await.unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 150);
    }
}
