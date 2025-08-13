//! QuinnListener and it's implements.
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::vec;

use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::{BoxStream, Stream, StreamExt};
use futures_util::task::noop_waker_ref;
use http::uri::Scheme;
use salvo_http3::quinn::{self, Endpoint};

use super::{QuinnAdapter, QuinnConnection};
use crate::conn::quinn::ServerConfig;
use crate::conn::{Accepted, Acceptor, Holding, IntoConfigStream, Listener};
use crate::fuse::{ArcFuseFactory, FuseInfo, TransProto};
use crate::http::Version;

/// A wrapper of `Listener` with quinn.
pub struct QuinnListener<S, C, T, E> {
    config_stream: S,
    local_addr: T,
    _phantom: PhantomData<(C, E)>,
}
impl<S, C, T: Debug, E> Debug for QuinnListener<S, C, T, E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuinnListener")
            .field("local_addr", &self.local_addr)
            .finish()
    }
}
impl<S, C, T, E> QuinnListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<ServerConfig, Error = E> + Send + 'static,
    T: ToSocketAddrs + Send,
    E: StdError + Send,
{
    /// Bind to socket address.
    #[inline]
    pub fn new(config_stream: S, local_addr: T) -> Self {
        QuinnListener {
            config_stream,
            local_addr,
            _phantom: PhantomData,
        }
    }
}
impl<S, C, T, E> Listener for QuinnListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<ServerConfig, Error = E> + Send + 'static,
    T: ToSocketAddrs + Send + 'static,
    E: StdError + Send + 'static,
{
    type Acceptor = QuinnAcceptor<BoxStream<'static, C>, C, C::Error>;

    fn try_bind(self) -> BoxFuture<'static, crate::Result<Self::Acceptor>> {
        async move {
            let Self {
                config_stream,
                local_addr,
                ..
            } = self;
            let socket = local_addr
                .to_socket_addrs()?
                .next()
                .ok_or_else(|| IoError::new(ErrorKind::AddrNotAvailable, "No address available"))?;
            Ok(QuinnAcceptor::new(
                config_stream.into_stream().boxed(),
                socket,
            ))
        }
        .boxed()
    }
}

/// A wrapper of `Acceptor` with quinn.
pub struct QuinnAcceptor<S, C, E> {
    config_stream: S,
    socket: SocketAddr,
    holdings: Vec<Holding>,
    endpoint: Option<Endpoint>,
    _phantom: PhantomData<(C, E)>,
}

impl<S, C, E> Debug for QuinnAcceptor<S, C, E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuinnAcceptor")
            .field("socket", &self.socket)
            .field("holdings", &self.holdings)
            .field("endpoint", &self.endpoint)
            .finish()
    }
}

impl<S, C, E> QuinnAcceptor<S, C, E>
where
    S: Stream<Item = C> + Send + 'static,
    C: TryInto<ServerConfig, Error = E> + Send + 'static,
    E: StdError + Send,
{
    /// Create a new `QuinnAcceptor`.
    pub fn new(config_stream: S, socket: SocketAddr) -> QuinnAcceptor<S, C, E> {
        let holding = Holding {
            local_addr: socket.into(),
            http_versions: vec![Version::HTTP_3],
            http_scheme: Scheme::HTTPS,
        };
        QuinnAcceptor {
            config_stream,
            socket,
            holdings: vec![holding],
            endpoint: None,
            _phantom: PhantomData,
        }
    }
}

impl<S, C, E> Acceptor for QuinnAcceptor<S, C, E>
where
    S: Stream<Item = C> + Send + Unpin + 'static,
    C: TryInto<ServerConfig, Error = E> + Send + 'static,
    E: StdError + Send,
{
    type Adapter = QuinnAdapter;
    type Stream = QuinnConnection;

    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    async fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> IoResult<Accepted<Self::Adapter, Self::Stream>> {
        let config = {
            let mut config = None;
            while let Poll::Ready(Some(item)) = Pin::new(&mut self.config_stream)
                .poll_next(&mut Context::from_waker(noop_waker_ref()))
            {
                config = Some(item);
            }
            config
        };
        if let Some(config) = config {
            let config = config
                .try_into()
                .map_err(|e| IoError::other(e.to_string()))?;
            let endpoint = Endpoint::server(config, self.socket)?;
            if self.endpoint.is_some() {
                tracing::info!("quinn config changed.");
            } else {
                tracing::info!("quinn config loaded.");
            }
            self.endpoint = Some(endpoint);
        }
        let endpoint = match &self.endpoint {
            Some(endpoint) => endpoint,
            None => return Err(IoError::other("quinn: invalid quinn config.")),
        };

        if let Some(new_conn) = endpoint.accept().await {
            let remote_addr = new_conn.remote_address();
            let local_addr = self.holdings[0].local_addr.clone();
            match new_conn.await {
                Ok(conn) => {
                    let fusewire = fuse_factory.map(|f| {
                        f.create(FuseInfo {
                            trans_proto: TransProto::Tcp,
                            remote_addr: remote_addr.into(),
                            local_addr: local_addr.clone(),
                        })
                    });
                    return Ok(Accepted {
                        adapter: QuinnAdapter,
                        stream: QuinnConnection::new(
                            quinn::Connection::new(conn),
                            fusewire.clone(),
                        ),
                        fusewire,
                        local_addr: self.holdings[0].local_addr.clone(),
                        remote_addr: remote_addr.into(),
                        http_scheme: self.holdings[0].http_scheme.clone(),
                    });
                }
                Err(e) => return Err(IoError::other(e.to_string())),
            }
        }
        Err(IoError::other("quinn accept error"))
    }
}
