//! openssl module
use std::io::{Error as IoError, Result as IoResult};
use std::marker::PhantomData;
use std::error::Error as StdError;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::task::noop_waker_ref;
use futures_util::stream::{Stream,BoxStream, StreamExt};
use http::uri::Scheme;
use openssl::ssl::{Ssl, SslAcceptor};
use tokio::io::ErrorKind;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_openssl::SslStream;
use tokio_util::sync::CancellationToken;

use super::SslAcceptorBuilder;

use crate::async_trait;
use crate::conn::{Accepted, Acceptor,HttpBuilder, Holding, IntoConfigStream, Listener};
use crate::http::{HttpConnection, Version};
use crate::service::HyperHandler;

/// OpensslListener
pub struct OpensslListener<S, C, T, E> {
    config_stream: S,
    inner: T,
    _phantom: PhantomData<(C, E)>,
}

impl<S, C, T, E> OpensslListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<SslAcceptorBuilder, Error = E> + Send + 'static,
    T: Listener + Send,
    E: StdError + Send,
{
    /// Create new OpensslListener with config stream.
    #[inline]
    pub fn new(config_stream: S, inner: T) -> Self {
        OpensslListener {
            config_stream,
            inner,
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<S, C, T, E> Listener for OpensslListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<SslAcceptorBuilder, Error = E> + Send + 'static,
    T: Listener + Send,
    T::Acceptor: Send + 'static,
    E: StdError + Send,
{
    type Acceptor = OpensslAcceptor<BoxStream<'static, C>, C, T::Acceptor, E>;

    async fn try_bind(self) -> IoResult<Self::Acceptor> {
        Ok(OpensslAcceptor::new(
            self.config_stream.into_stream().boxed(),
            self.inner.try_bind().await?,
        ))
    }
}

/// OpensslAcceptor
pub struct OpensslAcceptor<S, C, T, E> {
    config_stream: S,
    inner: T,
    holdings: Vec<Holding>,
    tls_acceptor: Option<Arc<SslAcceptor>>,
    _phantom: PhantomData<(C, E)>,
}
impl<S, C, T, E> OpensslAcceptor<S, C, T, E>
where
    S: Stream<Item = C> + Send + 'static,
    C: TryInto<SslAcceptorBuilder, Error = E> + Send + 'static,
    T: Acceptor + Send,
    E: StdError + Send,
{
    /// Create new OpensslAcceptor.
    pub fn new(config_stream: S, inner: T) -> OpensslAcceptor<S, C, T, E> {
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
        OpensslAcceptor {
            config_stream,
            inner,
            holdings,
            tls_acceptor: None,
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<S> HttpConnection for SslStream<S>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
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

#[async_trait]
impl<S, C, T, E> Acceptor for OpensslAcceptor<S, C, T, E>
where
    S: Stream<Item = C> + Send + Unpin + 'static,
    C: TryInto<SslAcceptorBuilder, Error = E> + Send + 'static,
    T: Acceptor + Send + 'static,
    E: StdError + Send,
{
    type Conn = SslStream<T::Conn>;

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
        if let Some(config) = config {
            match config.try_into() {
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

        let Accepted {
            conn,
            local_addr,
            remote_addr,
            http_version,
            http_scheme,
        } = self.inner.accept().await?;
        let ssl = Ssl::new(tls_acceptor.context()).map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
        let mut tls_stream =
            SslStream::new(ssl, conn).map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
        use std::pin::Pin;
        Pin::new(&mut tls_stream)
            .accept()
            .await
            .map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
        Ok(Accepted {
            conn: tls_stream,
            local_addr,
            remote_addr,
            http_version,
            http_scheme,
        })
    }
}
