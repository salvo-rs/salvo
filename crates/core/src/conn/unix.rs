//! UnixListener module
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::path::Path;
use std::sync::Arc;

use tokio::net::{UnixListener as TokioUnixListener, UnixStream};

use crate::async_trait;
use crate::conn::HttpBuilders;
use crate::conn::{AppProto, LocalAddr, TransProto};
use crate::http::{HttpConnection, Version};
use crate::service::HyperHandler;

use super::{Accepted, Acceptor, IntoAcceptor, Listener};

/// Unix domain socket listener.
#[cfg(unix)]
pub struct UnixListener<T> {
    path: T,
}
#[cfg(unix)]
impl<T> UnixListener<T> {
    /// Creates a new `UnixListener` bind to the specified path.
    #[inline]
    pub fn bind(path: T) -> UnixListener<T> {
        UnixListener { path }
    }
}

#[async_trait]
impl<T> IntoAcceptor for UnixListener<T>
where
    T: AsRef<Path> + Send,
{
    type Acceptor = UnixAcceptor;
    async fn into_acceptor(self) -> IoResult<Self::Acceptor> {
        let inner = TokioUnixListener::bind(self.path)?;
        let local_addr = LocalAddr::new(inner.local_addr()?.into(), TransProto::Tcp, AppProto::Http);
        Ok(UnixAcceptor { inner, local_addr })
    }
}

#[cfg(unix)]
impl<T> Listener for UnixListener<T> where T: AsRef<Path> + Send {}

/// UnixAcceptor
pub struct UnixAcceptor {
    inner: TokioUnixListener,
    local_addr: LocalAddr,
}

#[cfg(unix)]
#[async_trait]
impl Acceptor for UnixAcceptor {
    type Conn = UnixStream;

    #[inline]
    fn local_addrs(&self) -> Vec<LocalAddr> {
        vec![self.local_addr.clone()]
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        self.inner.accept().await.map(move |(conn, remote_addr)| Accepted {
            conn,
            local_addr: self.local_addr.clone(),
            remote_addr: remote_addr.into(),
        })
    }
}

#[async_trait]
impl HttpConnection for UnixStream {
    async fn version(&mut self) -> Option<Version> {
        Some(Version::HTTP_11)
    }
    async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()> {
        builders
            .http1
            .serve_connection(self, handler)
            .with_upgrades()
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    impl Stream for UnixListener {
        type Item = Result<UnixStream, IoError>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.poll_accept(cx)
        }
    }
    #[tokio::test]
    async fn test_unix_listener() {
        let sock_file = "/tmp/test-salvo.sock";
        let mut listener = UnixListener::bind(sock_file);

        tokio::spawn(async move {
            let mut stream = tokio::net::UnixStream::connect(sock_file).await.unwrap();
            stream.write_i32(518).await.unwrap();
        });

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 518);
        std::fs::remove_file(sock_file).unwrap();
    }
}
