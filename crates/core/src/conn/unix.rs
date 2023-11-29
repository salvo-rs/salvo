//! UnixListener module
use std::fs::{set_permissions, Permissions};
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use http::uri::Scheme;
use tokio::net::{UnixListener as TokioUnixListener, UnixStream};
use tokio_util::sync::CancellationToken;        
use nix::unistd::{Gid, chown, Uid};    

use crate::async_trait;
use crate::conn::{Holding, HttpBuilder};
use crate::http::{HttpConnection, Version};
use crate::service::HyperHandler;

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
        UnixListener { path,
            permissions: None,
            owner: None, }
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
        self.owner =  Some((uid.map(|v| Uid::from_raw(v)), gid.map(|v| Gid::from_raw(v))));
        self
    }
}

#[async_trait]
impl<T> Listener for UnixListener<T>
where
    T: AsRef<Path> + Send + Clone,
{
    type Acceptor = UnixAcceptor;

    async fn try_bind(self) -> IoResult<Self::Acceptor> {
        let inner = match (self.permissions, self.owner) {
            (Some(permissions), Some((uid, gid))) => {
                let inner = TokioUnixListener::bind(self.path.clone())?;
                set_permissions(self.path.clone(), permissions)?;
                chown(self.path.as_ref().as_os_str().into(), uid, gid)?;
                inner
            }
            (Some(permissions), None) => {
                let inner = TokioUnixListener::bind(self.path.clone())?;
                set_permissions(self.path.clone(), permissions)?;
                inner
            }
            (None, Some((uid, gid))) => {
                let inner = TokioUnixListener::bind(self.path.clone())?;
                chown(self.path.as_ref().as_os_str().into(), uid, gid)?;
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
#[async_trait]
impl Acceptor for UnixAcceptor {
    type Conn = UnixStream;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        self.inner.accept().await.map(move |(conn, remote_addr)| Accepted {
            conn,
            local_addr: self.holdings[0].local_addr.clone(),
            remote_addr: remote_addr.into(),
            http_version: Version::HTTP_11,
            http_scheme: Scheme::HTTP,
        })
    }
}

#[async_trait]
impl HttpConnection for UnixStream {
    async fn serve(
        self,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        server_shutdown_token: CancellationToken,
        idle_connection_timeout: Option<Duration>,
    ) -> IoResult<()> {
        builder
            .serve_connection(self, handler, server_shutdown_token, idle_connection_timeout)
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))
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

        let Accepted { mut conn, .. } = acceptor.accept().await.unwrap();
        assert_eq!(conn.read_i32().await.unwrap(), 518);
        std::fs::remove_file(sock_file).unwrap();
    }
}
