//! openssl module
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, Result as IoResult};
use std::marker::PhantomData;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::{BoxStream, Stream, StreamExt};
use futures_util::task::noop_waker_ref;
use http::uri::Scheme;
use openssl::ssl::{Ssl, SslAcceptor};
use tokio_openssl::SslStream;

use super::SslAcceptorBuilder;

use crate::conn::{
    Accepted, Acceptor, HandshakeStream, Holding, IntoConfigStream, Listener,
};
use crate::conn::tcp::TcpAdapter;
use crate::fuse::ArcFuseFactory;
use crate::http::HttpAdapter;

/// OpensslListener
pub struct OpensslListener<S, C, T, E> {
    config_stream: S,
    inner: T,
    _phantom: PhantomData<(C, E)>,
}
impl<S, C, T: Debug, E> Debug for OpensslListener<S, C, T, E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpensslListener")
            .field("inner", &self.inner)
            .finish()
    }
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

impl<S, C, T, E> Listener for OpensslListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<SslAcceptorBuilder, Error = E> + Send + 'static,
    T: Listener + Send + 'static,
    T::Acceptor: Send + 'static,
    E: StdError + Send + 'static,
{
    type Acceptor = OpensslAcceptor<BoxStream<'static, C>, C, T::Acceptor, E>;

    fn try_bind(self) -> BoxFuture<'static, crate::Result<Self::Acceptor>> {
        async move {
            Ok(OpensslAcceptor::new(
                self.config_stream.into_stream().boxed(),
                self.inner.try_bind().await?,
            ))
        }
        .boxed()
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
impl<S, C, T, E> Debug for OpensslAcceptor<S, C, T, E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpensslAcceptor")
            .field("holdings", &self.holdings)
            .finish()
    }
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
                #[allow(unused_mut)]
                let mut versions = h.http_versions.clone();
                #[cfg(feature = "http1")]
                if !versions.contains(&crate::http::Version::HTTP_11) {
                    versions.push(crate::http::Version::HTTP_11);
                }
                #[cfg(feature = "http2")]
                if !versions.contains(&crate::http::Version::HTTP_2) {
                    versions.push(crate::http::Version::HTTP_2);
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

    /// Get the inner `Acceptor`.
    pub fn inner(&self) -> &T {
        &self.inner
    }
}

impl<S, C, T, E> Acceptor for OpensslAcceptor<S, C, T, E>
where
    S: Stream<Item = C> + Send + Unpin + 'static,
    C: TryInto<SslAcceptorBuilder, Error = E> + Send + 'static,
    T: Acceptor + Send + 'static,
    E: StdError + Send,
{
    type Adapter = TcpAdapter<Self::Stream>;
    type Stream = HandshakeStream<SslStream<T::Stream>>;

    /// Get the local address bound to this listener.
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    async fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> IoResult<Accepted<Self::Adapter, Self::Stream>> {
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
            None => return Err(IoError::other("openssl: tls_acceptor is none.")),
        };

        let Accepted {
            adapter: _,
            stream,
            fusewire,
            local_addr,
            remote_addr,
            ..
        } = self.inner.accept(fuse_factory).await?;
        let conn = async move {
            let ssl = Ssl::new(tls_acceptor.context()).map_err(IoError::other)?;
            let mut tls_stream = SslStream::new(ssl, stream).map_err(IoError::other)?;
            std::pin::Pin::new(&mut tls_stream)
                .accept()
                .await
                .map_err(IoError::other)?;
            Ok(tls_stream)
        };

        Ok(Accepted {
            adapter: TcpAdapter::new(),
            stream: HandshakeStream::new(conn, fusewire.clone()),
            fusewire,
            local_addr,
            remote_addr,
            http_scheme: Scheme::HTTPS,
        })
    }
}
