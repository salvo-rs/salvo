//! QuinnListener and it's implements.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::vec;

use futures_util::stream::{BoxStream, Stream, StreamExt};
use futures_util::task::noop_waker_ref;
use http::uri::Scheme;
use salvo_http3::http3_quinn::{self, Endpoint};

use crate::async_trait;
use crate::conn::quinn::ServerConfig;
use crate::conn::{Holding, IntoConfigStream};
use crate::http::Version;

use super::H3Connection;
use crate::conn::{Accepted, Acceptor, Listener};

/// QuinnListener
pub struct QuinnListener<S, C, T> {
    config_stream: S,
    local_addr: T,
    _config: PhantomData<C>,
}
impl<S, C, T> QuinnListener<S, C, T>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<ServerConfig, Error = IoError> + Send + 'static,
    T: ToSocketAddrs + Send,
{
    /// Bind to socket address.
    #[inline]
    pub fn new(config_stream: S, local_addr: T) -> Self {
        QuinnListener {
            config_stream,
            local_addr,
            _config: PhantomData,
        }
    }
}
#[async_trait]
impl<S, C, T> Listener for QuinnListener<S, C, T>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<ServerConfig, Error = IoError> + Send + 'static,
    T: ToSocketAddrs + Send,
{
    type Acceptor = QuinnAcceptor<BoxStream<'static, C>, C>;

    async fn try_bind(self) -> IoResult<Self::Acceptor> {
        let Self {
            config_stream,
            local_addr,
            ..
        } = self;
        let socket = local_addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| IoError::new(ErrorKind::AddrNotAvailable, "No address available"))?;
        Ok(QuinnAcceptor::new(config_stream.into_stream().boxed(), socket))
    }
}

/// QuinnAcceptor
pub struct QuinnAcceptor<S, C> {
    config_stream: S,
    socket: SocketAddr,
    holdings: Vec<Holding>,
    endpoint: Option<Endpoint>,
    _config: PhantomData<C>,
}

impl<S, C> QuinnAcceptor<S, C>
where
    S: Stream<Item = C> + Send + 'static,
    C: TryInto<ServerConfig, Error = IoError> + Send + 'static,
{
    /// Create a new `QuinnAcceptor`.
    pub fn new(config_stream: S, socket: SocketAddr) -> QuinnAcceptor<S, C> {
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
            _config: PhantomData,
        }
    }
}

#[async_trait]
impl<S, C> Acceptor for QuinnAcceptor<S, C>
where
    S: Stream<Item = C> + Send + Unpin + 'static,
    C: TryInto<ServerConfig, Error = IoError> + Send + 'static,
{
    type Conn = H3Connection;

    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        let config = {
            let mut config = None;
            while let Poll::Ready(Some(item)) =
                Pin::new(&mut self.config_stream).poll_next(&mut Context::from_waker(noop_waker_ref()))
            {
                config = Some(item);
            }
            config
        };
        if let Some(config) = config {
            let endpoint = Endpoint::server(config.try_into()?, self.socket)?;
            if self.endpoint.is_some() {
                tracing::info!("quinn config changed.");
            } else {
                tracing::info!("quinn config loaded.");
            }
            self.endpoint = Some(endpoint);
        }
        let endpoint = match &self.endpoint {
            Some(endpoint) => endpoint,
            None => return Err(IoError::new(ErrorKind::Other, "quinn: invalid quinn config.")),
        };

        if let Some(new_conn) = endpoint.accept().await {
            let remote_addr = new_conn.remote_address();
            match new_conn.await {
                Ok(conn) => {
                    let conn = http3_quinn::Connection::new(conn);
                    return Ok(Accepted {
                        conn: H3Connection(conn),
                        local_addr: self.holdings[0].local_addr.clone(),
                        remote_addr: remote_addr.into(),
                        http_scheme: self.holdings[0].http_scheme.clone(),
                        http_version: Version::HTTP_3,
                    });
                }
                Err(e) => return Err(IoError::new(ErrorKind::Other, e.to_string())),
            }
        }
        Err(IoError::new(ErrorKind::Other, "quinn accept error"))
    }
}
