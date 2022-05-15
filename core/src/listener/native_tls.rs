//! tls module
use std::fmt::{self, Formatter};
use std::future::Future;
use std::io::{self, Cursor, Error as IoError, ErrorKind, Read};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::future::Ready;
use futures_util::{ready, stream, Stream};
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_native_tls::native_tls::{Identity, TlsAcceptor};
use tokio_native_tls::{TlsAcceptor as AsyncTlsAcceptor, TlsStream};

use super::{IntoAddrIncoming, LazyFile, Listener};
use crate::addr::SocketAddr;
use crate::transport::Transport;

/// Builder to set the configuration for the TLS server.
pub struct NativeTlsConfig {
    pkcs12: Box<dyn Read + Send + Sync>,
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
            pkcs12: Box::new(io::empty()),
            password: String::new(),
        }
    }

    /// Sets the pkcs12 via File Path, returns [`std::io::Error`] if the file cannot be open
    #[inline]
    pub fn with_pkcs12_path(mut self, path: impl AsRef<Path>) -> Self {
        self.pkcs12 = Box::new(LazyFile {
            path: path.as_ref().into(),
            file: None,
        });
        self
    }

    /// Sets the pkcs12 via bytes slice
    #[inline]
    pub fn with_pkcs12(mut self, pkcs12: impl Into<Vec<u8>>) -> Self {
        self.pkcs12 = Box::new(Cursor::new(pkcs12.into()));
        self
    }
    /// Sets the password
    #[inline]
    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = password.into();
        self
    }

    /// generate identity
    #[inline]
    pub fn identity(mut self) -> Result<Identity, IoError> {
        let mut pkcs12 = Vec::new();
        self.pkcs12
            .read_to_end(&mut pkcs12)
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))?;
        Identity::from_pkcs12(&pkcs12, &self.password).map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))
    }
}

pin_project! {
    /// NativeTlsListener
    pub struct NativeTlsListener<C> {
        #[pin]
        config_stream: C,
        incoming: AddrIncoming,
        identity: Option<Identity>,
    }
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
        Ok(Self::with_identity(identity))
    }
    /// Create new NativeTlsListenerBuilder with Identity.
    #[inline]
    pub fn with_identity(identity: impl Into<Identity>) -> NativeTlsListenerBuilder<stream::Once<Ready<Identity>>> {
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
    /// Get local address
    pub fn local_addr(&self) -> SocketAddr {
        self.incoming.local_addr().into()
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
            let identity = identity.into();
            *this.identity = Some(identity);
        }
        if let Some(identity) = this.identity {
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

pin_project! {
    /// NativeTlsStream
    pub struct NativeTlsStream {
        // #[pin]
        // acceptor: Pin<Box<AsyncTlsAcceptor>>,
        #[pin]
        inner_future: Pin<Box<dyn Future<Output=Result<TlsStream<AddrStream>, tokio_native_tls::native_tls::Error>> + Send>>,
        inner_stream: Option<TlsStream<AddrStream>>,
        remote_addr: SocketAddr,
    }
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
            // acceptor: Box::pin(acceptor),
            inner_future: Box::pin(async move { acceptor.accept(stream).await }),
            inner_stream: None,
            remote_addr,
        })
    }
}

impl AsyncRead for NativeTlsStream {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let mut this = self.project();
        if let Some(inner_stream) = &mut this.inner_stream {
            Pin::new(inner_stream).poll_read(cx, buf)
        } else if let Ok(stream) = ready!(this.inner_future.poll(cx)) {
            *this.inner_stream = Some(stream);
            Pin::new(this.inner_stream.as_mut().unwrap()).poll_read(cx, buf)
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
        } else if let Ok(stream) = ready!(this.inner_future.poll(cx)) {
            *this.inner_stream = Some(stream);
            Pin::new(this.inner_stream.as_mut().unwrap()).poll_write(cx, buf)
        } else {
            Poll::Ready(Err(IoError::new(ErrorKind::Other, "native tls error")))
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut this = self.project();
        if let Some(inner_stream) = &mut this.inner_stream {
            Pin::new(inner_stream).poll_flush(cx)
        } else if let Ok(stream) = ready!(this.inner_future.poll(cx)) {
            *this.inner_stream = Some(stream);
            Pin::new(this.inner_stream.as_mut().unwrap()).poll_flush(cx)
        } else {
            Poll::Ready(Err(IoError::new(ErrorKind::Other, "native tls error")))
        }
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut this = self.project();
        if let Some(inner_stream) = &mut this.inner_stream {
            Pin::new(inner_stream).poll_shutdown(cx)
        } else if let Ok(stream) = ready!(this.inner_future.poll(cx)) {
            *this.inner_stream = Some(stream);
            Pin::new(this.inner_stream.as_mut().unwrap()).poll_shutdown(cx)
        } else {
            Poll::Ready(Err(IoError::new(ErrorKind::Other, "native tls error")))
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
        C: Stream,
        C::Item: Into<Identity>,
    {
        type Item = Result<NativeTlsStream, IoError>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.poll_accept(cx)
        }
    }

    #[tokio::test]
    async fn test_native_tls_listener() {
        let addr = "127.0.0.1:7879";
        let mut listener = NativeTlsListener::with_config(
            NativeTlsConfig::new()
                .with_pkcs12(include_bytes!("../../certs/identity.p12").to_vec())
                .with_password("mypass"),
        )
        .bind(addr);
        tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            let connector = tokio_native_tls::TlsConnector::from(
                tokio_native_tls::native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .build()
                    .unwrap(),
            );
            let mut tls_stream = connector.connect(addr, stream).await.unwrap();
            tls_stream.write_i32(518).await.unwrap();
        });

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 518);
    }
}
