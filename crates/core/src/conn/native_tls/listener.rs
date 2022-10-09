//! native_tls module
use std::fmt::{self, Formatter};
use std::fs::File;
use std::future::Future;
use std::io::{self, Error as IoError, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::future::Ready;
use futures_util::{ready, stream, Stream};
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener as TokioTcpListener, ToSocketAddrs};
use tokio_native_tls::native_tls::{Identity, TlsAcceptor};
use tokio_native_tls::{TlsAcceptor as AsyncTlsAcceptor, TlsStream};

use crate::async_trait;
use crate::conn::{Accepted, Acceptor, HandshakeStream, IntoConfigStream, Listener, SocketAddr, TcpListener};

use super::{NativeTlsConfig, NativeTlsStream};

/// NativeTlsListener
#[pin_project]
pub struct NativeTlsListener<C, T> {
    #[pin]
    config_stream: C,
    identity: Option<Identity>,
    inner: T,
    current_tls_acceptor: Option<tokio_native_tls::TlsAcceptor>,
}

impl<C> NativeTlsListener<C, TcpListener>
where
    C: IntoConfigStream<Identity>,
{
    /// Bind to socket address.
    #[inline]
    pub async fn bind(config_stream: C, addr: impl ToSocketAddrs) -> NativeTlsListener<TcpListener, C> {
        Self::try_bind(config, addr).unwrap()
    }
    /// Try to bind to socket address.
    #[inline]
    pub async fn try_bind(config_stream: C, addr: impl ToSocketAddrs) -> IoResult<NativeTlsListener<TcpListener, C>> {
        let inner = TcpListener::bind(addr).await?;
        Ok(NativeTlsListener {
            config_stream,
            identity: None,
            inner,
            current_tls_acceptor: None,
        })
    }
}

impl From<NativeTlsConfig> for Identity {
    fn from(config: NativeTlsConfig) -> Self {
        config.identity().unwrap()
    }
}
impl<C, T> NativeTlsListener<C, T>
where
    C: IntoConfigStream<Identity>,
{
    /// Create new NativeTlsListener with config stream.
    #[inline]
    pub fn new(config_stream: C, inner: T) -> Self<C, T> {
        Self {
            config_stream,
            inner,
            identity: None,
            current_tls_acceptor: None,
        }
    }
}

#[async_trait]
impl<C> Acceptor for NativeTlsListener<C>
where
    C: IntoConfigStream<Identity>,
{
    type Conn = HandshakeStream<TlsStream<T::Conn>>;
    type Error = IoError;

    #[inline]
    fn local_addrs(&self) -> Vec<&SocketAddr> {
        self.inner.local_addrs()
    }

    #[inline]
    async fn accept(&self) -> Result<Accepted<Self::Conn>, Self::Error> {
        loop {
            tokio::select! {
                res = self.config_stream.next() => {
                    if let Some(tls_config) = res {
                        match tls_config.create_acceptor() {
                            Ok(acceptor) => {
                                if self.current_tls_acceptor.is_some() {
                                    tracing::info!("tls config changed.");
                                } else {
                                    tracing::info!("tls config loaded.");
                                }
                                self.current_tls_acceptor = Some(tokio_native_tls::TlsAcceptor::from(acceptor));
                            },
                            Err(err) => tracing::error!(error = %err, "invalid tls config."),
                        }
                    } else {
                        unreachable!()
                    }
                }
                res = self.inner.accept() => {
                    let (stream, local_addr, remote_addr, _) = res?;
                    let tls_acceptor = match &self.current_tls_acceptor {
                        Some(tls_acceptor) => tls_acceptor.clone(),
                        None => return Err(IoError::new(ErrorKind::Other, "no valid tls config.")),
                    };
                    let fut = async move { tls_acceptor.accept(stream).map_err(|err| IoError::new(ErrorKind::Other, err.to_string())).await };
                    let stream = HandshakeStream::new(fut);
                    return Ok(Accepted {
                        stream, local_addr, remote_addr});
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;
    impl<C> Stream for NativeTlsListener<C>
    where
        C: Stream + Send + Unpin + 'static,
        C::Item: Into<Identity>,
    {
        type Item = Result<NativeTlsStream, IoError>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.poll_accept(cx)
        }
    }
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
