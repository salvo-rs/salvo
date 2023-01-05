//! openssl module
use std::io::{Error as IoError, Result as IoResult};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::stream::BoxStream;
use futures_util::task::noop_waker_ref;
use futures_util::{Stream, StreamExt};
use http::uri::Scheme;
use openssl::ssl::{Ssl, SslAcceptor};
use tokio::io::ErrorKind;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_openssl::SslStream;

use super::OpensslConfig;

use crate::async_trait;
use crate::conn::Holding;
use crate::conn::{Accepted, Acceptor, HttpBuilders, IntoConfigStream, Listener, TlsConnStream};
use crate::http::{version_from_alpn, HttpConnection, Version};
use crate::service::HyperHandler;

/// OpensslListener
pub struct OpensslListener<C, T> {
    config_stream: C,
    inner: T,
}

impl<C, T> OpensslListener<C, T>
where
    C: IntoConfigStream<OpensslConfig> + Send + 'static,
    T: Listener + Send,
{
    /// Create new OpensslListener with config stream.
    #[inline]
    pub fn new(config_stream: C, inner: T) -> Self {
        OpensslListener { config_stream, inner }
    }
}

#[async_trait]
impl<C, T> Listener for OpensslListener<C, T>
where
    C: IntoConfigStream<OpensslConfig> + Send + 'static,
    T: Listener + Send,
    T::Acceptor: Send + 'static,
{
    type Acceptor = OpensslAcceptor<BoxStream<'static, OpensslConfig>, T::Acceptor>;

    async fn bind(self) -> Self::Acceptor {
        self.try_bind().await.unwrap()
    }

    async fn try_bind(self) -> IoResult<Self::Acceptor> {
        Ok(OpensslAcceptor::new(
            self.config_stream.into_stream().boxed(),
            self.inner.try_bind().await?,
        ))
    }
}

/// OpensslAcceptor
pub struct OpensslAcceptor<C, T> {
    config_stream: C,
    inner: T,
    holdings: Vec<Holding>,
    tls_acceptor: Option<Arc<SslAcceptor>>,
}
impl<C, T> OpensslAcceptor<C, T>
where
    T: Acceptor,
{
    /// Create new OpensslAcceptor.
    pub fn new(config_stream: C, inner: T) -> OpensslAcceptor<C, T> {
        let holdings = inner
            .holdings()
            .iter()
            .map(|h| Holding {
                local_addr: h.local_addr.clone(),
                http_version: Version::HTTP_2,
                http_scheme: Scheme::HTTPS,
            })
            .collect();
        OpensslAcceptor {
            config_stream,
            inner,
            holdings,
            tls_acceptor: None,
        }
    }
}

#[async_trait]
impl<S> HttpConnection for SslStream<S>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    async fn version(&mut self) -> Option<Version> {
        self.ssl().selected_alpn_protocol().map(version_from_alpn)
    }
    async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()> {
        builders
            .http2
            .serve_connection(self, handler)
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))
    }
}

#[async_trait]
impl<C, T> Acceptor for OpensslAcceptor<C, T>
where
    C: Stream<Item = OpensslConfig> + Send + Unpin + 'static,
    T: Acceptor + Send + 'static,
{
    type Conn = TlsConnStream<SslStream<T::Conn>>;

    /// Get the local address bound to this listener.
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
        if let Some(mut config) = config {
            match config.create_acceptor_builder() {
                Ok(builder) => {
                    if self.tls_acceptor.is_some() {
                        tracing::info!("tls config changed.");
                    } else {
                        tracing::info!("tls config loaded.");
                    }
                    self.tls_acceptor = Some(Arc::new(builder.build()));
                }
                Err(e) => tracing::error!(error = ?e, "openssl: invalid tls config."),
            }
        }
        let tls_acceptor = match &self.tls_acceptor {
            Some(tls_acceptor) => tls_acceptor.clone(),
            None => return Err(IoError::new(ErrorKind::Other, "openssl: invalid tls config.")),
        };
        let accepted = self.inner.accept().await?.map_conn(|stream| {
            let fut = async move {
                let ssl =
                    Ssl::new(tls_acceptor.context()).map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
                let mut tls_stream =
                    SslStream::new(ssl, stream).map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
                use std::pin::Pin;
                Pin::new(&mut tls_stream)
                    .accept()
                    .await
                    .map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
                Ok(tls_stream)
            };
            TlsConnStream::new(fut)
        });
        Ok(accepted)
    }
}
