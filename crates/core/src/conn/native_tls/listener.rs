//! native_tls module
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::stream::BoxStream;
use futures_util::task::noop_waker_ref;
use futures_util::{Stream, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::ToSocketAddrs;
use tokio_native_tls::TlsStream;

use crate::async_trait;
use crate::conn::{
    Accepted, Acceptor, HttpBuilders, IntoConfigStream, Listener, SocketAddr, TcpListener, TlsConnStream,
};
use crate::http::version::{self, HttpConnection, Version};
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
    async fn into_acceptor(self) -> IoResult<Self::Acceptor> {
        Ok(NativeTlsAcceptor::new(
            self.config_stream.into_stream().boxed(),
            self.inner.into_acceptor().await?,
        ))
    }
}

impl<C, T> NativeTlsListener<C, TcpListener<T>>
where
    C: IntoConfigStream<NativeTlsConfig>,
    T: ToSocketAddrs + Send + 'static,
{
    /// Bind to socket address.
    #[inline]
    pub fn bind(config: C, addr: T) -> NativeTlsListener<C::Stream, TcpListener<T>> {
        NativeTlsListener {
            config_stream: config.into_stream(),
            inner: TcpListener::bind(addr),
        }
    }
}

#[async_trait]
impl<S> HttpConnection for TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    async fn http_version(&mut self) -> Option<Version> {
        self.get_ref().negotiated_alpn().ok().flatten().map(version::from_alpn)
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
    tls_acceptor: Option<tokio_native_tls::TlsAcceptor>,
}
impl<C, T> NativeTlsAcceptor<C, T> {
    /// Create a new `NativeTlsAcceptor`.
    pub fn new(config_stream: C, inner: T) -> NativeTlsAcceptor<C, T> {
        NativeTlsAcceptor {
            config_stream,
            inner,
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
    fn local_addrs(&self) -> Vec<&SocketAddr> {
        self.inner.local_addrs()
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
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

    #[tokio::test]
    async fn test_native_tls_listener() {
        let mut listener = NativeTlsListener::with_config(
            NativeTlsConfig::new()
                .with_pkcs12(include_bytes!("../../certs/identity.p12").as_ref())
                .with_password("mypass"),
        )
        .bind("127.0.0.1:0");
        let addr = listener.local_addr();

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

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 10);
    }
}
