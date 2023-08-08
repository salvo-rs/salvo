//! native_tls module
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::stream::BoxStream;
use futures_util::task::noop_waker_ref;
use futures_util::{Stream, StreamExt};
use http::uri::Scheme;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_native_tls::TlsStream;
use tokio_util::sync::CancellationToken;

use crate::async_trait;
use crate::conn::{Accepted, Acceptor, Holding, HttpBuilder, IntoConfigStream, Listener};
use crate::http::{HttpConnection, Version};
use crate::service::HyperHandler;

use super::NativeTlsConfig;

/// NativeTlsListener
pub struct NativeTlsListener<C, T> {
    config_stream: C,
    inner: T,
}
impl<C, T> NativeTlsListener<C, T>
where
    C: IntoConfigStream<NativeTlsConfig> + Send + 'static,
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
    C: IntoConfigStream<NativeTlsConfig> + Send + 'static,
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
            .map(|h| {
                let mut versions = h.http_versions.clone();
                #[cfg(feature = "http1")]
                if !versions.contains(&Version::HTTP_11) {
                    versions.push(Version::HTTP_11);
                }
                #[cfg(feature = "http2")]
                if !versions.contains(&Version::HTTP_2) {
                    versions.push(Version::HTTP_2);
                }
                Holding {
                    local_addr: h.local_addr.clone(),
                    http_versions: versions,
                    http_scheme: Scheme::HTTPS,
                }
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
    type Conn = TlsStream<T::Conn>;

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
                Err(e) => tracing::error!(error = ?e, "native_tls: invalid tls config"),
            }
        }

        let tls_acceptor = match &self.tls_acceptor {
            Some(tls_acceptor) => tls_acceptor.clone(),
            None => return Err(IoError::new(ErrorKind::Other, "native_tls: invalid tls config")),
        };
        let Accepted {
            conn,
            local_addr,
            remote_addr,
            http_version,
            http_scheme,
        } = self.inner.accept().await?;
        let conn = tls_acceptor
            .accept(conn)
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))?;
        Ok(Accepted {
            conn,
            local_addr,
            remote_addr,
            http_version,
            http_scheme,
        })
    }
}
