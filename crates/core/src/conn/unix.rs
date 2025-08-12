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

use crate::{async_trait, Error};
use crate::conn::{Holding, StraightStream};
use crate::fuse::{ArcFuseFactory, FuseInfo, TransProto};
use crate::http::Version;

use super::{Accepted, Acceptor, Listener};

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
    pub fn new(path: T) -> UnixListener<T> {
        UnixListener {
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
        pub fn backlog(mut self, backlog: u32) -> Self {
            self.backlog = Some(backlog);
            self
        }
    }
}

#[async_trait]
impl<T> Listener for UnixListener<T>
where
    T: AsRef<Path> + Send + Clone,
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
        Ok(UnixAcceptor { inner, holdings })
    }
}

/// `UnixAcceptor` is used to accept a Unix socket connection.
#[derive(Debug)]
pub struct UnixAcceptor {
    inner: TokioUnixListener,
    holdings: Vec<Holding>,
}

impl UnixAcceptor {
    /// Get the inner `TokioUnixListener`.
    pub fn inner(&self) -> &TokioUnixListener {
        &self.inner
    }
}

#[cfg(unix)]
#[async_trait]
impl Acceptor for UnixAcceptor {
    type Conn = StraightStream<UnixStream>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> IoResult<Accepted<Self::Conn>> {
        self.inner.accept().await.map(move |(conn, remote_addr)| {
            let remote_addr = Arc::new(remote_addr);
            let local_addr = self.holdings[0].local_addr.clone();
            Accepted {
                conn: StraightStream::new(
                    conn,
                    fuse_factory.map(|f| {
                        f.create(FuseInfo {
                            trans_proto: TransProto::Tcp,
                            remote_addr: remote_addr.clone().into(),
                            local_addr: local_addr.clone(),
                        })
                    }),
                ),
                local_addr: self.holdings[0].local_addr.clone(),
                remote_addr: remote_addr.into(),
                http_scheme: Scheme::HTTP,
            }
        })
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

        let Accepted { mut conn, .. } = acceptor.accept(None).await.unwrap();
        assert_eq!(conn.read_i32().await.unwrap(), 518);
        std::fs::remove_file(sock_file).unwrap();
    }
}
