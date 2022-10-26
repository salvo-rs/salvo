//! rustls module
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::stream::BoxStream;
use futures_util::task::noop_waker_ref;
use futures_util::{Stream, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::ToSocketAddrs;
use tokio_rustls::server::TlsStream;

use crate::async_trait;
use crate::conn::{AppProto, LocalAddr};
use crate::conn::{Accepted, Acceptor, HttpBuilders, IntoConfigStream, Listener, TcpListener, TlsConnStream, IntoAcceptor};
use crate::http::{version_from_alpn, HttpConnection, Version};
use crate::service::HyperHandler;

use super::RustlsConfig;

/// RustlsListener

pub struct RustlsListener<C, T> {
    config_stream: C,
    inner: T,
}

impl<C, T> RustlsListener<C, T>
where
    C: IntoConfigStream<RustlsConfig>,
    T: Listener + Send,
{
    /// Create a new `RustlsListener`.
    #[inline]
    pub fn new(config_stream: C, inner: T) -> Self {
        RustlsListener { config_stream, inner }
    }
}

#[async_trait]
impl<C, T> IntoAcceptor for RustlsListener<C, T>
where
    C: IntoConfigStream<RustlsConfig>,
    T: IntoAcceptor + Send,
    T::Acceptor: Send + 'static,
{
    type Acceptor = RustlsAcceptor<BoxStream<'static, RustlsConfig>, T::Acceptor>;
    async fn into_acceptor(self) -> IoResult<Self::Acceptor> {
        Ok(RustlsAcceptor::new(
            self.config_stream.into_stream().boxed(),
            self.inner.into_acceptor().await?,
        ))
    }
}
impl<C, T> Listener for RustlsListener<C, T>
where
    C: IntoConfigStream<RustlsConfig>,
    T: IntoAcceptor + Send,
    T::Acceptor: Send + 'static, {}

impl<C, T> RustlsListener<C, TcpListener<T>>
where
    C: IntoConfigStream<RustlsConfig>,
    T: ToSocketAddrs + Send + 'static,
{
    /// Bind to socket address.
    #[inline]
    pub fn bind(config_stream: C, local_addr: T) -> RustlsListener<C, TcpListener<T>> {
        RustlsListener {
            config_stream,
            inner: TcpListener::bind(local_addr),
        }
    }
}

/// RustlsAcceptor
pub struct RustlsAcceptor<C, T> {
    config_stream: C,
    inner: T,
    local_addrs: Vec<LocalAddr>,
    tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
}
impl<C, T> RustlsAcceptor<C, T>
where
    T: Acceptor,
{
    /// Create a new `RustlsAcceptor`.
    pub fn new(config_stream: C, inner: T) -> RustlsAcceptor<C, T> {
        let local_addrs = inner
            .local_addrs()
            .iter()
            .map(|l| LocalAddr::new(l.addr.clone(), l.trans_proto, AppProto::Https))
            .collect();
        RustlsAcceptor {
            config_stream,
            inner,
            local_addrs,
            tls_acceptor: None,
        }
    }
}

#[async_trait]
impl<S> HttpConnection for TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    async fn version(&mut self) -> Option<Version> {
        self.get_ref().1.alpn_protocol().map(version_from_alpn)
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
impl<C, T> Acceptor for RustlsAcceptor<C, T>
where
    C: Stream<Item = RustlsConfig> + Send + Unpin + 'static,
    T: Acceptor + Send + 'static,
    <T as Acceptor>::Conn: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Conn = TlsConnStream<TlsStream<T::Conn>>;

    fn local_addrs(&self) -> Vec<LocalAddr> {
        self.local_addrs.clone()
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        let config = {
            let mut config = None;
            while let Poll::Ready(Some(item)) =
                Pin::new(&mut self.config_stream).poll_next(&mut Context::from_waker(noop_waker_ref()))
            {
                config = Some(item);
            }
            config
        };
        if let Some(config) = config {
            let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config.build_server_config()?));
            if self.tls_acceptor.is_some() {
                tracing::info!("tls config changed.");
            } else {
                tracing::info!("tls config loaded.");
            }
            self.tls_acceptor = Some(tls_acceptor);
        }
        let tls_acceptor = match &self.tls_acceptor {
            Some(tls_acceptor) => tls_acceptor,
            None => return Err(IoError::new(ErrorKind::Other, "rustls: invalid tls config.")),
        };

        let accepted = self
            .inner
            .accept()
            .await?
            .map_conn(|s| TlsConnStream::new(tls_acceptor.accept(s)));

        Ok(accepted)
    }
}
