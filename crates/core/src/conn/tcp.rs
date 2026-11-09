//! TcpListener and its implementations.
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, Result as IoResult};
use std::net::SocketAddr;
use std::sync::Arc;
use std::vec;

use futures_util::future::{BoxFuture, FutureExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener as TokioTcpListener, TcpStream, ToSocketAddrs};
use tokio_util::sync::CancellationToken;

use super::admission::PendingAdmission;
use super::{Accepted, Acceptor, Coupler, DynStream, Listener};
#[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl"))]
use crate::conn::IntoConfigStream;
#[cfg(feature = "native-tls")]
use crate::conn::native_tls::NativeTlsListener;
#[cfg(feature = "openssl")]
use crate::conn::openssl::OpensslListener;
#[cfg(feature = "rustls")]
use crate::conn::rustls::RustlsListener;
use crate::conn::{Holding, HttpBuilder, StraightStream};
use crate::fuse::{ArcFusePolicy, FuseInfo, TransProto};
use crate::http::Version;
use crate::http::uri::Scheme;
use crate::service::HyperHandler;

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
        Self {
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
    #[must_use]
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
    // Both the socket and the admission future survive a cancelled accept call.
    pending: Option<PendingAdmission<TcpStream>>,
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

        Ok(Self {
            inner,
            holdings,
            pending: None,
        })
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
        fuse_policy: Option<ArcFusePolicy>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        loop {
            if self.pending.is_none() {
                let (conn, remote_addr) = self.inner.accept().await?;
                self.pending = Some(PendingAdmission::new(
                    conn,
                    FuseInfo {
                        trans_proto: TransProto::Tcp,
                        remote_addr: remote_addr.into(),
                        local_addr: self.holdings[0].local_addr.clone(),
                    },
                    fuse_policy.clone(),
                ));
            }
            let decision = self
                .pending
                .as_mut()
                .expect("pending set above")
                .decide()
                .await;
            let pending = self.pending.take().expect("pending set above");
            let Some(admitted) = decision else { continue };
            let conn_ctrl = pending.conn_ctrl;
            let fuse_config = admitted.config;
            return Ok(Accepted {
                coupler: TcpCoupler::new(),
                stream: StraightStream::new(
                    pending.incoming,
                    fuse_config,
                    conn_ctrl.clone(),
                    admitted.observer,
                ),
                fuse_config,
                conn_ctrl,
                remote_addr: pending.info.remote_addr,
                local_addr: pending.info.local_addr,
                http_scheme: Scheme::HTTP,
            });
        }
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
        let fuse_config = handler.fuse_config;
        let conn_ctrl = handler.conn_ctrl.clone();
        async move {
            builder
                .serve_connection(stream, handler, fuse_config, conn_ctrl, graceful_stop_token)
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
        fuse_policy: Option<ArcFusePolicy>,
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
        fuse_policy: Option<ArcFusePolicy>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        DynTcpAcceptor::accept(self, fuse_policy).await
    }
}

/// Convert an `Acceptor` into a boxed `DynTcpAcceptor`.
pub struct ToDynTcpAcceptor<A>(pub A);
impl<A> DynTcpAcceptor for ToDynTcpAcceptor<A>
where
    A: Acceptor + 'static,
    A::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    #[inline]
    fn holdings(&self) -> &[Holding] {
        self.0.holdings()
    }

    #[inline]
    fn accept(
        &mut self,
        fuse_policy: Option<ArcFusePolicy>,
    ) -> BoxFuture<'_, IoResult<Accepted<TcpCoupler<DynStream>, DynStream>>> {
        async move {
            let accepted = self.0.accept(fuse_policy).await?;
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
        fuse_policy: Option<ArcFusePolicy>,
    ) -> BoxFuture<'_, IoResult<Accepted<TcpCoupler<DynStream>, DynStream>>> {
        async move {
            let mut set = Vec::new();
            for inner in &mut self.inners {
                let fuse_policy = fuse_policy.clone();
                set.push(async move { inner.accept(fuse_policy).await }.boxed());
            }
            futures_util::future::select_all(set).await.0
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
        fuse_policy: Option<ArcFusePolicy>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        DynTcpAcceptor::accept(self, fuse_policy).await
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::{Notify, Semaphore};

    use super::*;
    use crate::conn::ConnCtrl;
    use crate::fuse::{ArcConnObserver, FuseAction, FuseConfig, FusePolicy};

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

    struct AdmissionState {
        entered: Notify,
        proceed: Semaphore,
        calls: AtomicUsize,
        cancelled: AtomicUsize,
        observed: AtomicUsize,
    }

    impl Default for AdmissionState {
        fn default() -> Self {
            Self {
                entered: Notify::new(),
                proceed: Semaphore::new(0),
                calls: Default::default(),
                cancelled: Default::default(),
                observed: Default::default(),
            }
        }
    }

    struct Attempt {
        state: Arc<AdmissionState>,
        complete: bool,
    }
    impl Drop for Attempt {
        fn drop(&mut self) {
            if !self.complete {
                self.state.cancelled.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    struct GatePolicy {
        state: Arc<AdmissionState>,
        slow_port: u16,
        reject: bool,
    }

    #[crate::async_trait]
    impl FusePolicy for GatePolicy {
        async fn decide(&self, info: &FuseInfo) -> FuseAction {
            if info.local_addr.clone().into_std().unwrap().port() == self.slow_port {
                let mut attempt = Attempt {
                    state: self.state.clone(),
                    complete: false,
                };
                self.state.calls.fetch_add(1, Ordering::SeqCst);
                self.state.entered.notify_one();
                self.state.proceed.acquire().await.unwrap().forget();
                attempt.complete = true;
                if self.reject {
                    return FuseAction::Reject;
                }
            }
            FuseAction::Accept(FuseConfig::disabled())
        }

        fn observe(&self, info: &FuseInfo, _ctrl: &ConnCtrl) -> Option<ArcConnObserver> {
            if info.local_addr.clone().into_std().unwrap().port() == self.slow_port {
                self.state.observed.fetch_add(1, Ordering::SeqCst);
            }
            None
        }
    }

    async fn pause_admission(
        acceptor: &mut TcpAcceptor,
        policy: ArcFusePolicy,
        state: &AdmissionState,
    ) {
        tokio::time::timeout(Duration::from_secs(3), async {
            tokio::select! {
                biased;
                _ = state.entered.notified() => {},
                _ = Acceptor::accept(acceptor, Some(policy)) => panic!("admission should wait for its gate"),
            }
        }).await.unwrap();
    }

    #[tokio::test]
    async fn cancelled_accept_resumes_original_policy() {
        let mut acceptor = TcpListener::new("127.0.0.1:0").bind().await;
        let addr = acceptor.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        let state = Arc::new(AdmissionState::default());
        let policy: ArcFusePolicy = Arc::new(GatePolicy {
            state: state.clone(),
            slow_port: addr.port(),
            reject: false,
        });
        pause_admission(&mut acceptor, policy, &state).await;
        assert_eq!(state.cancelled.load(Ordering::SeqCst), 0);
        state.proceed.add_permits(1);
        // Removing the policy on the next call does not discard the first decision.
        let accepted = tokio::time::timeout(Duration::from_secs(3), acceptor.accept(None))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            accepted.remote_addr.into_std().unwrap(),
            client.local_addr().unwrap()
        );
        assert_eq!(accepted.fuse_config, Some(FuseConfig::disabled()));
        assert_eq!(state.calls.load(Ordering::SeqCst), 1);
        assert_eq!(state.observed.load(Ordering::SeqCst), 1);
        assert_eq!(state.cancelled.load(Ordering::SeqCst), 0);
        // A later connection does use the new policy argument.
        let _next_client = TcpStream::connect(addr).await.unwrap();
        let next = tokio::time::timeout(Duration::from_secs(3), acceptor.accept(None))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next.fuse_config, None);
    }

    #[tokio::test]
    async fn dropping_acceptor_cancels_admission_and_closes_socket() {
        let mut acceptor = TcpListener::new("127.0.0.1:0").bind().await;
        let addr = acceptor.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).await.unwrap();
        let state = Arc::new(AdmissionState::default());
        let policy: ArcFusePolicy = Arc::new(GatePolicy {
            state: state.clone(),
            slow_port: addr.port(),
            reject: false,
        });
        pause_admission(&mut acceptor, policy, &state).await;
        assert_eq!(state.cancelled.load(Ordering::SeqCst), 0);
        drop(acceptor);
        assert_eq!(state.cancelled.load(Ordering::SeqCst), 1);
        let read = tokio::time::timeout(Duration::from_secs(3), client.read_u8())
            .await
            .unwrap();
        assert_eq!(read.unwrap_err().kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn resumed_rejection_closes_peer_and_accepts_next_connection() {
        let mut acceptor = TcpListener::new("127.0.0.1:0").bind().await;
        let addr = acceptor.local_addr().unwrap();
        let mut first = TcpStream::connect(addr).await.unwrap();
        let state = Arc::new(AdmissionState::default());
        let policy: ArcFusePolicy = Arc::new(GatePolicy {
            state: state.clone(),
            slow_port: addr.port(),
            reject: true,
        });
        pause_admission(&mut acceptor, policy, &state).await;
        let second = TcpStream::connect(addr).await.unwrap();
        state.proceed.add_permits(1);
        let accepted = tokio::time::timeout(Duration::from_secs(3), acceptor.accept(None))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            accepted.remote_addr.into_std().unwrap(),
            second.local_addr().unwrap()
        );
        assert_eq!(state.calls.load(Ordering::SeqCst), 1);
        assert_eq!(state.observed.load(Ordering::SeqCst), 0);
        assert_eq!(state.cancelled.load(Ordering::SeqCst), 0);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(3), first.read_u8())
                .await
                .unwrap()
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }

    async fn assert_composed_admission_resumes<A: Acceptor + 'static>(
        mut acceptor: A,
        slow: SocketAddr,
        fast: SocketAddr,
    ) {
        let slow_client = TcpStream::connect(slow).await.unwrap();
        let state = Arc::new(AdmissionState::default());
        let policy: ArcFusePolicy = Arc::new(GatePolicy {
            state: state.clone(),
            slow_port: slow.port(),
            reject: false,
        });
        let driver = tokio::spawn(async move {
            for _ in 0..3 {
                let accepted = acceptor.accept(Some(policy.clone())).await.unwrap();
                assert_eq!(accepted.local_addr.into_std().unwrap(), fast);
            }
            acceptor
        });
        tokio::time::timeout(Duration::from_secs(3), state.entered.notified())
            .await
            .unwrap();
        let mut fast_clients = Vec::new();
        for _ in 0..3 {
            fast_clients.push(TcpStream::connect(fast).await.unwrap());
        }
        let mut acceptor = tokio::time::timeout(Duration::from_secs(3), driver)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            state.calls.load(Ordering::SeqCst),
            1,
            "same peer must not restart its decision"
        );
        assert_eq!(state.cancelled.load(Ordering::SeqCst), 0);
        state.proceed.add_permits(1);
        let accepted = tokio::time::timeout(Duration::from_secs(3), acceptor.accept(None))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            accepted.remote_addr.into_std().unwrap(),
            slow_client.local_addr().unwrap()
        );
        assert_eq!(state.calls.load(Ordering::SeqCst), 1);
        assert_eq!(state.observed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn joined_accept_resumes_admission_after_other_listener_wins() {
        let slow = TcpListener::new("127.0.0.1:0").bind().await;
        let fast = TcpListener::new("127.0.0.1:0").bind().await;
        let slow_addr = slow.local_addr().unwrap();
        let fast_addr = fast.local_addr().unwrap();
        assert_composed_admission_resumes(
            crate::conn::JoinedAcceptor::new(slow, fast),
            slow_addr,
            fast_addr,
        )
        .await;
    }

    #[tokio::test]
    async fn dynamic_accept_resumes_admission_after_other_listener_wins() {
        let slow = TcpListener::new("127.0.0.1:0").bind().await;
        let fast = TcpListener::new("127.0.0.1:0").bind().await;
        let slow_addr = slow.local_addr().unwrap();
        let fast_addr = fast.local_addr().unwrap();
        assert_composed_admission_resumes(
            DynTcpAcceptors::new(vec![slow.into_boxed(), fast.into_boxed()]),
            slow_addr,
            fast_addr,
        )
        .await;
    }
}
