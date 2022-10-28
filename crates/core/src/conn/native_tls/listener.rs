//! native_tls module
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::stream::BoxStream;
use futures_util::task::noop_waker_ref;
use futures_util::{Stream, StreamExt};
use http::uri::Scheme;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_native_tls::TlsStream;

use crate::async_trait;
use crate::conn::Holding;
use crate::conn::{Accepted, Acceptor, HttpBuilders, IntoConfigStream, Listener, TlsConnStream};
use crate::http::{version_from_alpn, HttpConnection, Version};
use crate::service::HyperHandler;

use super::NativeTlsConfig;

/// NativeTlsListener
pub struct NativeTlsListener<C, T> {
    config_stream: C,
    inner: T,
}
impl<C, T> NativeTlsListener<C, T>
where
    C: IntoConfigStream<NativeTlsConfig>,
    T: Listener + Send,
{
    /// Create a new `NativeTlsListener`.
    #[inline]
    pub fn new(config_stream: C, inner: T) -> Self {
        NativeTlsListener { config_stream, inner }
    }
}

#[async_trait]
impl<C, T> Listener for NativeTlsListener<C, T>
where
    C: IntoConfigStream<NativeTlsConfig>,
    T: Listener + Send,
    T::Acceptor: Send + 'static,
{
    type Acceptor = NativeTlsAcceptor<BoxStream<'static, NativeTlsConfig>, T::Acceptor>;

    async fn bind(self) -> Self::Acceptor {
        self.try_bind().await.unwrap()
    }

    async fn try_bind(self) -> IoResult<Self::Acceptor> {
        Ok(NativeTlsAcceptor::new(
            self.config_stream.into_stream().boxed(),
            self.inner.try_bind().await?,
        ))
    }
}

#[async_trait]
impl<S> HttpConnection for TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    async fn version(&mut self) -> Option<Version> {
        self.get_ref().negotiated_alpn().ok().flatten().map(version_from_alpn)
    }
    async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()> {
        builders
            .http2
            .serve_connection(self, handler)
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))
    }
}

/// NativeTlsAcceptor
pub struct NativeTlsAcceptor<C, T> {
    config_stream: C,
    inner: T,
    holdings: Vec<Holding>,
    tls_acceptor: Option<tokio_native_tls::TlsAcceptor>,
}
impl<C, T> NativeTlsAcceptor<C, T>
where
    T: Acceptor,
{
    /// Create a new `NativeTlsAcceptor`.
    pub fn new(config_stream: C, inner: T) -> NativeTlsAcceptor<C, T> {
        let holdings = inner
            .holdings()
            .iter()
            .map(|h| Holding {
                local_addr: h.local_addr.clone(),
                http_version: Version::HTTP_2,
                http_scheme: Scheme::HTTPS,
            })
            .collect();
        NativeTlsAcceptor {
            config_stream,
            inner,
            holdings,
            tls_acceptor: None,
        }
    }
}

#[async_trait]
impl<C, T> Acceptor for NativeTlsAcceptor<C, T>
where
    C: Stream<Item = NativeTlsConfig> + Send + Unpin + 'static,
    T: Acceptor + Send + 'static,
    <T as Acceptor>::Conn: AsyncRead + AsyncWrite + Unpin + Send,
{
    type Conn = TlsConnStream<TlsStream<T::Conn>>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        let config = {
            let mut config = None;
            while let Poll::Ready(Some(item)) = self
                .config_stream
                .poll_next_unpin(&mut Context::from_waker(noop_waker_ref()))
            {
                config = Some(item);
            }
            config
        };
        if let Some(config) = config {
            let identity = config.identity()?;
            let tls_acceptor = tokio_native_tls::native_tls::TlsAcceptor::new(identity);
            match tls_acceptor {
                Ok(tls_acceptor) => {
                    if self.tls_acceptor.is_some() {
                        tracing::info!("tls config changed.");
                    } else {
                        tracing::info!("tls config loaded.");
                    }
                    self.tls_acceptor = Some(tokio_native_tls::TlsAcceptor::from(tls_acceptor));
                }
                Err(e) => tracing::error!(error = ?e, "native_tls: invalid tls config."),
            }
        }

        let tls_acceptor = match &self.tls_acceptor {
            Some(tls_acceptor) => tls_acceptor.clone(),
            None => return Err(IoError::new(ErrorKind::Other, "native_tls: invalid tls config.")),
        };
        let accepted = self.inner.accept().await?.map_conn(|conn| {
            let fut = async move {
                tls_acceptor
                    .accept(conn)
                    .await
                    .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))
            };
            TlsConnStream::new(fut)
        });
        Ok(accepted)
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;
    use crate::conn::TcpListener;

    #[tokio::test]
    async fn test_native_tls_listener() {
        let mut acceptor = TcpListener::new("127.0.0.1:0")
            .native_tls(
                NativeTlsConfig::new()
                    .with_pkcs12(include_bytes!("../../../certs/identity.p12").as_ref())
                    .with_password("mypass"),
            )
            .bind()
            .await;
        let addr = acceptor.holdings()[0].local_addr.clone().into_std().unwrap();

        tokio::spawn(async move {
            let connector = tokio_native_tls::TlsConnector::from(
                tokio_native_tls::native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .build()
                    .unwrap(),
            );
            let stream = TcpStream::connect(addr).await.unwrap();
            let mut stream = connector.connect("127.0.0.1", stream).await.unwrap();
            stream.write_i32(10).await.unwrap();
        });

        let Accepted { mut conn, .. } = acceptor.accept().await.unwrap();
        assert_eq!(conn.read_i32().await.unwrap(), 10);
    }
}
