//! UnixListener module
use std::io::Result as IoResult;
use std::path::Path;
use std::sync::Arc;

use http::uri::Scheme;
use tokio::net::{UnixListener as TokioUnixListener, UnixStream};

use crate::async_trait;
use crate::conn::Holding;
use crate::conn::HttpBuilders;
use crate::http::{HttpConnection, Version};
use crate::service::HyperHandler;

use super::{Accepted, Acceptor, Listener};

/// Unix domain socket listener.
#[cfg(unix)]
pub struct UnixListener<T> {
    path: T,
}
#[cfg(unix)]
impl<T> UnixListener<T> {
    /// Creates a new `UnixListener` bind to the specified path.
    #[inline]
    pub fn new(path: T) -> UnixListener<T> {
        UnixListener { path }
    }
}

#[async_trait]
impl<T> Listener for UnixListener<T>
where
    T: AsRef<Path> + Send,
{
    type Acceptor = UnixAcceptor;

    async fn bind(self) -> Self::Acceptor {
        self.try_bind().await.unwrap()
    }

    async fn try_bind(self) -> IoResult<Self::Acceptor> {
        let inner = TokioUnixListener::bind(self.path)?;
        let holding = Holding {
            local_addr: inner.local_addr()?.into(),
            http_version: Version::HTTP_11,
            http_scheme: Scheme::HTTP,
        };
        Ok(UnixAcceptor {
            inner,
            holdings: vec![holding],
        })
    }
}

/// UnixAcceptor
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
            http_version: self.holdings[0].http_version,
            http_scheme: self.holdings[0].http_scheme.clone(),
        })
    }
}

#[async_trait]
impl HttpConnection for UnixStream {
    async fn version(&mut self) -> Option<Version> {
        Some(Version::HTTP_11)
    }
    async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()> {
        #[cfg(not(feature = "http1"))]
        {
            let _ = handler;
            let _ = builders;
            panic!("http1 feature is required");
        }
        #[cfg(feature = "http1")]
        builders
            .http1
            .serve_connection(self, handler)
            .with_upgrades()
            .await
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
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
