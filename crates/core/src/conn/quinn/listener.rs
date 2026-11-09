//! QuinnListener and its implementations.
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::marker::PhantomData;
use std::net::{SocketAddr, ToSocketAddrs};
use std::vec;

use futures_util::stream::{BoxStream, StreamExt};
use http::uri::Scheme;
use salvo_http3::quinn::Endpoint;
use salvo_http3::quinn::quinn::Incoming;
use tokio_util::sync::CancellationToken;

use super::{QuinnConnection, QuinnCoupler};
use crate::Error;
use crate::conn::admission::PendingAdmission;
use crate::conn::quinn::ServerConfig;
use crate::conn::{Accepted, Acceptor, Holding, IntoConfigStream, Listener};
use crate::fuse::{ArcFusePolicy, FuseInfo, TransProto};
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
        Self {
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
    type Acceptor = QuinnAcceptor;

    async fn try_bind(self) -> crate::Result<Self::Acceptor> {
        let Self {
            config_stream,
            local_addr,
            ..
        } = self;
        let socket = local_addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| IoError::new(ErrorKind::AddrNotAvailable, "No address available"))?;

        let mut config_stream = config_stream.into_stream().boxed();
        let initial = config_stream.next().await.ok_or_else(|| {
            Error::other("quinn: config stream ended before yielding an initial tls config")
        })?;
        let initial = initial
            .try_into()
            .map_err(|err| IoError::other(err.to_string()))?;
        let endpoint = Endpoint::server(initial, socket)?;
        let cancel_reload = CancellationToken::new();

        tracing::info!("quinn config loaded");
        tokio::spawn(reload_configs(
            config_stream,
            endpoint.clone(),
            cancel_reload.clone(),
        ));

        Ok(QuinnAcceptor::new(endpoint, socket, cancel_reload))
    }
}

async fn reload_configs<C, E>(
    mut config_stream: BoxStream<'static, C>,
    endpoint: Endpoint,
    cancel_reload: CancellationToken,
) where
    C: TryInto<ServerConfig, Error = E> + Send + 'static,
    E: StdError + Send + 'static,
{
    loop {
        tokio::select! {
            _ = cancel_reload.cancelled() => break,
            next = config_stream.next() => {
                let Some(config) = next else {
                    break;
                };
                match config.try_into() {
                    Ok(config) => {
                        endpoint.set_server_config(Some(config));
                        tracing::info!("quinn config changed");
                    }
                    Err(err) => {
                        tracing::error!(error = ?err, "quinn: invalid tls config, keeping previous config");
                    }
                }
            }
        }
    }
}

/// A wrapper of `Acceptor` with quinn.
pub struct QuinnAcceptor {
    socket: SocketAddr,
    holdings: Vec<Holding>,
    endpoint: Endpoint,
    cancel_reload: CancellationToken,
    pending: Option<PendingAdmission<Incoming>>,
}

impl Debug for QuinnAcceptor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuinnAcceptor")
            .field("socket", &self.socket)
            .field("holdings", &self.holdings)
            .field("endpoint", &self.endpoint)
            .finish()
    }
}

impl QuinnAcceptor {
    /// Create a new `QuinnAcceptor`.
    #[must_use]
    pub fn new(endpoint: Endpoint, socket: SocketAddr, cancel_reload: CancellationToken) -> Self {
        let holding = Holding {
            local_addr: socket.into(),
            http_versions: vec![Version::HTTP_3],
            http_scheme: Scheme::HTTPS,
        };
        Self {
            socket,
            holdings: vec![holding],
            endpoint,
            cancel_reload,
            pending: None,
        }
    }
}

impl Drop for QuinnAcceptor {
    fn drop(&mut self) {
        self.cancel_reload.cancel();
    }
}

impl Acceptor for QuinnAcceptor {
    type Coupler = QuinnCoupler;
    type Stream = QuinnConnection;

    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    async fn accept(
        &mut self,
        fuse_policy: Option<ArcFusePolicy>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        loop {
            if self.pending.is_none() {
                let incoming = self
                    .endpoint
                    .accept()
                    .await
                    .ok_or_else(|| IoError::other("quinn accept error"))?;
                let info = FuseInfo {
                    trans_proto: TransProto::Quic,
                    remote_addr: incoming.remote_address().into(),
                    local_addr: self.holdings[0].local_addr.clone(),
                };
                self.pending = Some(PendingAdmission::new(incoming, info, fuse_policy.clone()));
            }
            let decision = self
                .pending
                .as_mut()
                .expect("incoming parked above")
                .decide()
                .await;
            let pending = self.pending.take().expect("incoming parked above");
            let Some(admitted) = decision else { continue };
            let fuse_config = admitted.config;
            // Admission passed. Hand the connection over without awaiting its handshake, which
            // `serve_connection` awaits on the connection's own task. Awaiting it here would stall
            // every other accept, TCP included via `JoinedListener`, and would lose the
            // connection whenever this future is dropped mid-handshake.
            let new_conn = pending.incoming;
            // Of the fuse timeouts, QUIC enforces the handshake timeout in `serve_connection` and
            // the request-body timeout via the H3 body. The transport idle and write-stall
            // timeouts are TCP/byte-stream concepts handled by `StraightStream`; QUIC relies on
            // quinn's own `max_idle_timeout` and per-stream flow control instead (see the
            // `FuseConfig` field docs).
            let handshake_timeout = fuse_config.and_then(|config| config.tls_handshake_timeout);
            let connecting = new_conn
                .accept()
                .map_err(|e| IoError::other(e.to_string()))?;
            let (conn, handshake_done) = connecting
                .into_0rtt()
                .map_err(|_| IoError::other("quinn refused 0.5-RTT on a server connection"))?;
            return Ok(Accepted {
                coupler: QuinnCoupler,
                stream: QuinnConnection::new(conn, handshake_done, handshake_timeout),
                fuse_config,
                conn_ctrl: pending.conn_ctrl,
                local_addr: pending.info.local_addr,
                remote_addr: pending.info.remote_addr,
                http_scheme: self.holdings[0].http_scheme.clone(),
            });
        }
    }
}
