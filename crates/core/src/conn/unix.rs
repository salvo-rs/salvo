//! UnixListener module
use std::fs::{set_permissions, Permissions};
use std::io::Result as IoResult;
use std::path::Path;
use std::sync::Arc;

use http::uri::Scheme;
use nix::unistd::{chown, Gid, Uid};
use tokio::net::{UnixListener as TokioUnixListener, UnixStream};

use crate::conn::{Holding, StraightStream};
use crate::fuse::{ArcFuseFactory,FuseInfo,TransProto};
use crate::http::Version;
use crate::Error;

use super::{Accepted, Acceptor, Listener};

/// `UnixListener` is used to create a Unix socket connection listener.
#[cfg(unix)]
pub struct UnixListener<T> {
    path: T,
    permissions: Option<Permissions>,
    owner: Option<(Option<Uid>, Option<Gid>)>,
}
#[cfg(unix)]
impl<T> UnixListener<T> {
    /// Creates a new `UnixListener` bind to the specified path.
    #[inline]
    pub fn new(path: T) -> UnixListener<T> {
        UnixListener {
            path,
            permissions: None,
            owner: None,
        }
    }

    /// Provides permissions to be set on actual bind.
    #[inline]
    pub fn permissions(mut self, permissions: impl Into<Option<Permissions>>) -> Self {
        self.permissions = permissions.into();
        self
    }

    #[inline]
    /// Provides owner to be set on actual bind.
    pub fn owner(mut self, uid: Option<u32>, gid: Option<u32>) -> Self {
        self.owner = Some((uid.map(Uid::from_raw), gid.map(Gid::from_raw)));
        self
    }
}

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
                set_permissions(self.path.clone(), permissions)?;
                inner
            }
            (None, Some((uid, gid))) => {
                let inner = TokioUnixListener::bind(self.path.clone())?;
                chown(self.path.as_ref().as_os_str(), uid, gid).map_err(Error::other)?;
                inner
            }
            (None, None) => TokioUnixListener::bind(self.path)?,
        };

        let holding = Holding {
            local_addr: inner.local_addr()?.into(),
            http_versions: vec![Version::HTTP_11],
            http_scheme: Scheme::HTTP,
        };
        Ok(UnixAcceptor {
            inner,
            holdings: vec![holding],
        })
    }
}

/// `UnixAcceptor` is used to accept a Unix socket connection.
pub struct UnixAcceptor {
    inner: TokioUnixListener,
    holdings: Vec<Holding>,
}

#[cfg(unix)]
impl Acceptor for UnixAcceptor {
    type Conn = StraightStream<UnixStream>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(&mut self, fuse_factory: Option<ArcFuseFactory>) -> IoResult<Accepted<Self::Conn>> {
        self.inner.accept().await.map(move |(conn, remote_addr)|{
            let remote_addr = Arc::new(remote_addr);
            let local_addr = self.holdings[0].local_addr.clone();
             Accepted {
            conn: StraightStream::new(conn, fuse_factory.map(|f|f.create(FuseInfo {
                trans_proto: TransProto::Tcp,
                remote_addr: remote_addr.clone().into(),
                local_addr: local_addr.clone()
            }))),
            local_addr: self.holdings[0].local_addr.clone(),
            remote_addr: remote_addr.clone().into(),
            http_version: Version::HTTP_11,
            http_scheme: Scheme::HTTP,
        }})
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
