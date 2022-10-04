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
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_native_tls::native_tls::{Identity, TlsAcceptor};
use tokio_native_tls::{TlsAcceptor as AsyncTlsAcceptor, TlsStream};

use super::{IntoAddrIncoming, Listener};
use crate::addr::SocketAddr;
use crate::transport::Transport;

/// Builder to set the configuration for the TLS server.
pub struct NativeTlsConfig {
    pkcs12_path: Option<PathBuf>,
    pkcs12: Vec<u8>,
    password: String,
}

impl fmt::Debug for NativeTlsConfig {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("NativeTlsConfig").finish()
    }
}

impl Default for NativeTlsConfig {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
impl NativeTlsConfig {
    /// Create new `NativeTlsConfig`
    #[inline]
    pub fn new() -> Self {
        NativeTlsConfig {
            pkcs12_path: None,
            pkcs12: vec![],
            password: String::new(),
        }
    }

    /// Sets the pkcs12 via File Path, returns [`std::io::Error`] if the file cannot be open
    #[inline]
    pub fn with_pkcs12_path(mut self, path: impl AsRef<Path>) -> Self {
        self.pkcs12_path = Some(path.as_ref().into());
        self
    }

    /// Sets the pkcs12 via bytes slice
    #[inline]
    pub fn with_pkcs12(mut self, pkcs12: impl Into<Vec<u8>>) -> Self {
        self.pkcs12 = pkcs12.into();
        self
    }
    /// Sets the password
    #[inline]
    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = password.into();
        self
    }

    /// Generate identity
    #[inline]
    pub fn identity(mut self) -> Result<Identity, IoError> {
        if self.pkcs12.is_empty() {
            if let Some(path) = &self.pkcs12_path {
                let mut file = File::open(path)?;
                file.read_to_end(&mut self.pkcs12)?;
            }
        }
        Identity::from_pkcs12(&self.pkcs12, &self.password).map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))
    }
}

/// NativeTlsListener
#[pin_project]
pub struct NativeTlsListener<C> {
    #[pin]
    config_stream: C,
    incoming: AddrIncoming,
    identity: Option<Identity>,
}

/// NativeTlsListener
pub struct NativeTlsListenerBuilder<C> {
    config_stream: C,
}
impl<C> NativeTlsListenerBuilder<C>
where
    C: Stream,
    C::Item: Into<Identity>,
{
    /// Bind to socket address.
    #[inline]
    pub fn bind(self, incoming: impl IntoAddrIncoming) -> NativeTlsListener<C> {
        self.try_bind(incoming).unwrap()
    }
    /// Try to bind to socket address.
    #[inline]
    pub fn try_bind(self, incoming: impl IntoAddrIncoming) -> Result<NativeTlsListener<C>, hyper::Error> {
        Ok(NativeTlsListener {
            config_stream: self.config_stream,
            incoming: incoming.into_incoming(),
            identity: None,
        })
    }
}

impl NativeTlsListener<stream::Once<Ready<Identity>>> {
    /// Create new NativeTlsListenerBuilder with NativeTlsConfig.
    #[inline]
    pub fn with_config(config: NativeTlsConfig) -> NativeTlsListenerBuilder<stream::Once<Ready<Identity>>> {
        Self::try_with_config(config).unwrap()
    }
    /// Try to create new NativeTlsListenerBuilder with NativeTlsConfig.
    #[inline]
    pub fn try_with_config(
        config: NativeTlsConfig,
    ) -> Result<NativeTlsListenerBuilder<stream::Once<Ready<Identity>>>, IoError> {
        let identity = config.identity()?;
        Ok(Self::identity(identity))
    }
    /// Create new NativeTlsListenerBuilder with Identity.
    #[inline]
    pub fn identity(identity: impl Into<Identity>) -> NativeTlsListenerBuilder<stream::Once<Ready<Identity>>> {
        let stream = futures_util::stream::once(futures_util::future::ready(identity.into()));
        Self::with_config_stream(stream)
    }
}

impl From<NativeTlsConfig> for Identity {
    fn from(config: NativeTlsConfig) -> Self {
        config.identity().unwrap()
    }
}
impl<C> NativeTlsListener<C> {
    /// Get the [`AddrIncoming] of this listener.
    #[inline]
    pub fn incoming(&self) -> &AddrIncoming {
        &self.incoming
    }

    /// Get the local address bound to this listener.
    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.incoming.local_addr()
    }
}
impl<C> NativeTlsListener<C>
where
    C: Stream,
    C::Item: Into<Identity>,
{
    /// Create new NativeTlsListener with config stream.
    #[inline]
    pub fn with_config_stream(config_stream: C) -> NativeTlsListenerBuilder<C> {
        NativeTlsListenerBuilder { config_stream }
    }
}

impl<C> Listener for NativeTlsListener<C>
where
    C: Stream,
    C::Item: Into<Identity>,
{
}
impl<C> Accept for NativeTlsListener<C>
where
    C: Stream,
    C::Item: Into<Identity>,
{
    type Conn = NativeTlsStream;
    type Error = IoError;

    #[inline]
    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let this = self.project();
        if let Poll::Ready(Some(identity)) = this.config_stream.poll_next(cx) {
            *this.identity = Some(identity.into());
        }
        if let Some(identity) = &this.identity {
            match ready!(Pin::new(this.incoming).poll_accept(cx)) {
                Some(Ok(sock)) => {
                    let stream = NativeTlsStream::new(sock.remote_addr().into(), sock, identity.clone())?;
                    Poll::Ready(Some(Ok(stream)))
                }
                Some(Err(e)) => Poll::Ready(Some(Err(e))),
                _ => Poll::Ready(None),
            }
        } else {
            Poll::Ready(Some(Err(IoError::new(ErrorKind::Other, "acceptor is none"))))
        }
    }
}

/// NativeTlsStream
#[pin_project]
pub struct NativeTlsStream {
    #[pin]
    inner_future: Pin<Box<dyn Future<Output=Result<TlsStream<AddrStream>, tokio_native_tls::native_tls::Error>> + Send>>,
    inner_stream: Option<TlsStream<AddrStream>>,
    remote_addr: SocketAddr,
    accepted: bool,
}
    
impl Transport for NativeTlsStream {
    #[inline]
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr.clone())
    }
}

impl NativeTlsStream {
    #[inline]
    fn new(remote_addr: SocketAddr, stream: AddrStream, identity: Identity) -> Result<Self, IoError> {
        let acceptor: AsyncTlsAcceptor = TlsAcceptor::new(identity)
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))?
            .into();
        Ok(NativeTlsStream {
            inner_future: Box::pin(async move { acceptor.accept(stream).await }),
            inner_stream: None,
            remote_addr,
            accepted: false,
        })
    }
}

impl AsyncRead for NativeTlsStream {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let mut this = self.project();
        if let Some(inner_stream) = &mut this.inner_stream {
            Pin::new(inner_stream).poll_read(cx, buf)
        } else if !*this.accepted {
            match this.inner_future.poll(cx) {
                Poll::Ready(Ok(stream)) => {
                    *this.accepted = true;
                    *this.inner_stream = Some(stream);
                    Pin::new(this.inner_stream.as_mut().unwrap()).poll_read(cx, buf)
                }
                Poll::Ready(Err(_)) => {
                    *this.accepted = true;
                    Poll::Ready(Err(IoError::new(ErrorKind::Other, "native tls error")))
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Err(IoError::new(ErrorKind::Other, "native tls error")))
        }
    }
}

impl AsyncWrite for NativeTlsStream {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let mut this = self.project();
        if let Some(inner_stream) = &mut this.inner_stream {
            Pin::new(inner_stream).poll_write(cx, buf)
        } else if !*this.accepted {
            match this.inner_future.poll(cx) {
                Poll::Ready(Ok(stream)) => {
                    *this.accepted = true;
                    *this.inner_stream = Some(stream);
                    Pin::new(this.inner_stream.as_mut().unwrap()).poll_write(cx, buf)
                }
                Poll::Ready(Err(_)) => {
                    *this.accepted = true;
                    Poll::Ready(Err(IoError::new(ErrorKind::Other, "native tls error")))
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Err(IoError::new(ErrorKind::Other, "native tls error")))
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut this = self.project();
        if let Some(inner_stream) = &mut this.inner_stream {
            Pin::new(inner_stream).poll_flush(cx)
        } else if !*this.accepted {
            match this.inner_future.poll(cx) {
                Poll::Ready(Ok(stream)) => {
                    *this.accepted = true;
                    *this.inner_stream = Some(stream);
                    Pin::new(this.inner_stream.as_mut().unwrap()).poll_flush(cx)
                }
                Poll::Ready(Err(_)) => {
                    *this.accepted = true;
                    Poll::Ready(Err(IoError::new(ErrorKind::Other, "native tls error")))
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Err(IoError::new(ErrorKind::Other, "native tls error")))
        }
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut this = self.project();
        if let Some(inner_stream) = &mut this.inner_stream {
            Pin::new(inner_stream).poll_shutdown(cx)
        } else {
            Poll::Ready(Ok(()))
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
