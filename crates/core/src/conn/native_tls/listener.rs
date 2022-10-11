//! native_tls module
use std::io::{Error as IoError, ErrorKind, Result as IoResult};

use futures_util::{Stream, StreamExt};
use pin_project::pin_project;
use tokio::net::ToSocketAddrs;
use tokio_native_tls::native_tls::Identity;
use tokio_native_tls::TlsStream;

use crate::async_trait;
use crate::conn::{Accepted, Acceptor, IntoConfigStream, SocketAddr, TcpListener, TlsConnStream};

/// NativeTlsListener
#[pin_project]
pub struct NativeTlsListener<C, T> {
    #[pin]
    config_stream: C,
    identity: Option<Identity>,
    inner: T,
    tls_acceptor: Option<tokio_native_tls::TlsAcceptor>,
}

impl<C> NativeTlsListener<C, TcpListener>
where
    C: IntoConfigStream<Identity> + Send + 'static,
{
    /// Bind to socket address.
    #[inline]
    pub async fn bind(config: C, addr: impl ToSocketAddrs) -> NativeTlsListener<C::Stream, TcpListener> {
        Self::try_bind(config, addr).await.unwrap()
    }
    /// Try to bind to socket address.
    #[inline]
    pub async fn try_bind(
        config: C,
        addr: impl ToSocketAddrs,
    ) -> IoResult<NativeTlsListener<C::Stream, TcpListener>> {
        let inner = TcpListener::try_bind(addr).await?;
        Ok(NativeTlsListener {
            config_stream: config.into_stream()?,
            identity: None,
            inner,
            tls_acceptor: None,
        })
    }
}

impl<C, T> NativeTlsListener<C, T>
where
    C: Stream + Send + 'static,
    C::Item: Into<Identity>,
{
    /// Create new NativeTlsListener with config stream.
    #[inline]
    pub fn new(config_stream: C, inner: T) -> Self {
        Self {
            config_stream,
            inner,
            identity: None,
            tls_acceptor: None,
        }
    }
}

#[async_trait]
impl<C, T> Acceptor for NativeTlsListener<C, T>
where
    C: Stream<Item = Identity> + Send + Unpin + 'static,
    T: Acceptor,
{
    type Conn = TlsConnStream<TlsStream<T::Conn>>;
    type Error = IoError;

    #[inline]
    fn local_addrs(&self) -> Vec<&SocketAddr> {
        self.inner.local_addrs()
    }

    #[inline]
    async fn accept(&mut self) -> Result<Accepted<Self::Conn>, Self::Error> {
        loop {
            tokio::select! {
                identity = self.config_stream.next() => {
                    if let Some(identity) = identity {
                        let tls_acceptor = tokio_native_tls::native_tls::TlsAcceptor::new(identity)
                            .map_err(|err| IoError::new(ErrorKind::Other, err.to_string()));
                        match tls_acceptor {
                            Ok(tls_acceptor) => {
                                if self.tls_acceptor.is_some() {
                                    tracing::info!("tls config changed.");
                                } else {
                                    tracing::info!("tls config loaded.");
                                }
                                self.tls_acceptor = Some(tokio_native_tls::TlsAcceptor::from(tls_acceptor));
                            },
                            Err(err) => tracing::error!(error = %err, "invalid tls config."),
                        }
                    } else {
                        unreachable!()
                    }
                }
                accepted = self.inner.accept() => {
                    let Accepted{stream, local_addr, remote_addr} = accepted.map_err(|e|IoError::new(ErrorKind::Other, format!("accept error: {}", e)))?;
                    let tls_acceptor = match &self.tls_acceptor {
                        Some(tls_acceptor) => tls_acceptor.clone(),
                        None => return Err(IoError::new(ErrorKind::Other, "no valid tls config.")),
                    };
                    let fut = async move { tls_acceptor.accept(stream).await.map_err(|err| IoError::new(ErrorKind::Other, err.to_string())) };
                    let stream = TlsConnStream::new(fut);
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
