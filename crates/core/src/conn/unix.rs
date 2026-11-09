//! UnixListener module
#[cfg(unix)]
use std::fmt::{self, Debug, Formatter};
use std::fs::{Permissions, set_permissions};
use std::io::Result as IoResult;
use std::path::Path;
use std::sync::Arc;

use http::uri::Scheme;
use nix::unistd::{Gid, Uid, chown};
use tokio::net::{UnixListener as TokioUnixListener, UnixStream};

use super::admission::PendingAdmission;
use super::{Accepted, Acceptor, Listener};
use crate::Error;
use crate::conn::tcp::{DynTcpAcceptor, TcpCoupler, ToDynTcpAcceptor};
use crate::conn::{Holding, StraightStream};
use crate::fuse::{ArcFusePolicy, FuseInfo, TransProto};
use crate::http::Version;

/// `UnixListener` is used to create a Unix socket connection listener.
#[cfg(unix)]
pub struct UnixListener<T> {
    path: T,
    permissions: Option<Permissions>,
    owner: Option<(Option<Uid>, Option<Gid>)>,
    #[cfg(feature = "socket2")]
    backlog: Option<u32>,
}

#[cfg(unix)]
impl<T: Debug> Debug for UnixListener<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnixListener")
            .field("path", &self.path)
            .field("permissions", &self.permissions)
            .field("owner", &self.owner)
            .finish()
    }
}

#[cfg(unix)]
impl<T> UnixListener<T> {
    /// Creates a new `UnixListener` bind to the specified path.
    #[cfg(not(feature = "socket2"))]
    #[inline]
    pub fn new(path: T) -> Self {
        Self {
            path,
            permissions: None,
            owner: None,
        }
    }
    /// Creates a new `UnixListener` bind to the specified path.
    #[cfg(feature = "socket2")]
    #[inline]
    pub fn new(path: T) -> Self {
        Self {
            path,
            permissions: None,
            owner: None,
            backlog: None,
        }
    }

    /// Provides permissions to be set on actual bind.
    #[inline]
    #[must_use]
    pub fn permissions(mut self, permissions: impl Into<Option<Permissions>>) -> Self {
        self.permissions = permissions.into();
        self
    }

    /// Provides owner to be set on actual bind.
    #[inline]
    #[must_use]
    pub fn owner(mut self, uid: Option<u32>, gid: Option<u32>) -> Self {
        self.owner = Some((uid.map(Uid::from_raw), gid.map(Gid::from_raw)));
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

impl<T> Listener for UnixListener<T>
where
    T: AsRef<Path> + Send + Clone + 'static,
{
    type Acceptor = UnixAcceptor;

    async fn try_bind(self) -> crate::Result<Self::Acceptor> {
        let inner = match (self.permissions, self.owner) {
            (Some(permissions), Some((uid, gid))) => {
                let inner = TokioUnixListener::bind(self.path.clone())?;
                set_permissions(self.path.clone(), permissions)?;
                chown(self.path.as_ref().as_os_str(), uid, gid).map_err(Error::other)?;
                inner
            }
            (Some(permissions), None) => {
                let inner = TokioUnixListener::bind(self.path.clone())?;
                set_permissions(self.path, permissions)?;
                inner
            }
            (None, Some((uid, gid))) => {
                let inner = TokioUnixListener::bind(self.path.clone())?;
                chown(self.path.as_ref().as_os_str(), uid, gid).map_err(Error::other)?;
                inner
            }
            (None, None) => TokioUnixListener::bind(self.path)?,
        };

        #[cfg(feature = "socket2")]
        if let Some(backlog) = self.backlog {
            let socket = socket2::SockRef::from(&inner);
            socket.listen(backlog as _)?;
        }

        let holdings = vec![Holding {
            local_addr: inner.local_addr()?.into(),
            #[cfg(not(feature = "http2-cleartext"))]
            http_versions: vec![Version::HTTP_11],
            #[cfg(feature = "http2-cleartext")]
            http_versions: vec![Version::HTTP_11, Version::HTTP_2],
            http_scheme: Scheme::HTTP,
        }];
        Ok(UnixAcceptor {
            inner,
            holdings,
            pending: None,
        })
    }
}

/// Used to accept a Unix socket connection.
#[derive(Debug)]
pub struct UnixAcceptor {
    inner: TokioUnixListener,
    holdings: Vec<Holding>,
    pending: Option<PendingAdmission<UnixStream>>,
}

impl UnixAcceptor {
    /// Convert the `UnixAcceptor` into a boxed `DynTcpAcceptor`.
    pub fn into_boxed(self) -> Box<dyn DynTcpAcceptor> {
        Box::new(ToDynTcpAcceptor(self))
    }
}

#[cfg(unix)]
impl Acceptor for UnixAcceptor {
    type Coupler = TcpCoupler<Self::Stream>;
    type Stream = StraightStream<UnixStream>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(
        &mut self,
        fuse_policy: Option<ArcFusePolicy>,
    ) -> IoResult<Accepted<TcpCoupler<Self::Stream>, Self::Stream>> {
        loop {
            if self.pending.is_none() {
                let (conn, remote_addr) = self.inner.accept().await?;
                self.pending = Some(PendingAdmission::new(
                    conn,
                    FuseInfo {
                        trans_proto: TransProto::Tcp,
                        remote_addr: Arc::new(remote_addr).into(),
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
                local_addr: pending.info.local_addr,
                remote_addr: pending.info.remote_addr,
                http_scheme: Scheme::HTTP,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::conn::{Accepted, Acceptor, Listener};

    #[tokio::test]
    async fn test_unix_listener() {
        let sock_file = "/tmp/test-salvo.sock";
        let mut acceptor = UnixListener::new(sock_file).bind().await;

        tokio::spawn(async move {
            let mut stream = tokio::net::UnixStream::connect(sock_file).await.unwrap();
            stream.write_i32(518).await.unwrap();
        });

        let Accepted { mut stream, .. } = acceptor.accept(None).await.unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 518);
        std::fs::remove_file(sock_file).unwrap();
    }

    #[tokio::test]
    async fn cancelled_accept_resumes_unix_admission() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        use tokio::sync::{Notify, Semaphore};

        use crate::fuse::{FuseAction, FuseConfig, FusePolicy};

        struct Policy {
            calls: AtomicUsize,
            entered: Notify,
            proceed: Semaphore,
        }
        #[crate::async_trait]
        impl FusePolicy for Policy {
            async fn decide(&self, _: &FuseInfo) -> FuseAction {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.entered.notify_one();
                self.proceed.acquire().await.unwrap().forget();
                FuseAction::Accept(FuseConfig::disabled())
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("admission.sock");
        let mut acceptor = UnixListener::new(path.clone()).bind().await;
        let mut client = UnixStream::connect(&path).await.unwrap();
        client.write_u8(42).await.unwrap();
        let policy = Arc::new(Policy {
            calls: AtomicUsize::new(0),
            entered: Notify::new(),
            proceed: Semaphore::new(0),
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            tokio::select! {
                biased;
                _ = policy.entered.notified() => {},
                _ = acceptor.accept(Some(policy.clone())) => panic!("admission should wait"),
            }
        })
        .await
        .unwrap();
        policy.proceed.add_permits(1);
        let mut accepted = tokio::time::timeout(Duration::from_secs(3), acceptor.accept(None))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(accepted.fuse_config, Some(FuseConfig::disabled()));
        assert_eq!(policy.calls.load(Ordering::SeqCst), 1);
        assert_eq!(accepted.stream.read_u8().await.unwrap(), 42);
    }
}
