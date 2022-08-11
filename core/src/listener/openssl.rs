//! openssl module
use std::fmt::{self, Formatter};
use std::io::{self, Cursor, Error as IoError, Read};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::Ready;
use futures_util::{ready, stream, Stream};
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use openssl::pkey::PKey;
use openssl::ssl::{Ssl, SslAcceptor, SslAcceptorBuilder, SslMethod, SslRef};
use openssl::x509::X509;
use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ErrorKind, ReadBuf};
use tokio_openssl::SslStream;

use super::{IntoAddrIncoming, LazyFile, Listener};
use crate::addr::SocketAddr;
use crate::transport::Transport;

/// Builder to set the configuration for the Tls server.
pub struct OpensslConfig {
    cert: Box<dyn Read + Send + Sync>,
    key: Box<dyn Read + Send + Sync>,
}

impl fmt::Debug for OpensslConfig {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("OpensslConfig").finish()
    }
}
impl Default for OpensslConfig {
    #[inline]
    fn default() -> Self {
        OpensslConfig::new()
    }
}

impl OpensslConfig {
    /// Create new `OpensslConfig`
    #[inline]
    pub fn new() -> Self {
        OpensslConfig {
            key: Box::new(io::empty()),
            cert: Box::new(io::empty()),
        }
    }

    /// Sets the Tls key via File Path, returns `Error::IoError` if the file cannot be open
    #[inline]
    pub fn with_key_path(mut self, path: impl AsRef<Path>) -> Self {
        self.key = Box::new(LazyFile {
            path: path.as_ref().into(),
            file: None,
        });
        self
    }

    /// Sets the Tls key via bytes slice
    #[inline]
    pub fn with_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.key = Box::new(Cursor::new(key.into()));
        self
    }

    /// Specify the file path for the TLS certificate to use.
    #[inline]
    pub fn with_cert_path(mut self, path: impl AsRef<Path>) -> Self {
        self.cert = Box::new(LazyFile {
            path: path.as_ref().into(),
            file: None,
        });
        self
    }

    /// Sets the Tls certificate via bytes slice
    #[inline]
    pub fn with_cert(mut self, cert: impl Into<Vec<u8>>) -> Self {
        self.cert = Box::new(Cursor::new(cert.into()));
        self
    }

    /// Create [`SslAcceptorBuilder`]
    pub fn create_acceptor_builder(mut self) -> Result<SslAcceptorBuilder, IoError> {
        let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;

        let mut cert_vec = Vec::new();
        self.cert.read_to_end(&mut cert_vec)?;
        let mut certs = X509::stack_from_pem(&cert_vec)?;
        let mut certs = certs.drain(..);
        builder.set_certificate(
            certs
                .next()
                .ok_or_else(|| IoError::new(ErrorKind::Other, "no leaf certificate"))?
                .as_ref(),
        )?;
        certs.try_for_each(|cert| builder.add_extra_chain_cert(cert))?;

        // convert it to Vec<u8> to allow reading it again if key is RSA
        let mut key_vec = Vec::new();
        self.key.read_to_end(&mut key_vec)?;

        if key_vec.is_empty() {
            return Err(IoError::new(ErrorKind::Other, "empty key"));
        }

        builder.set_private_key(PKey::private_key_from_pem(&key_vec)?.as_ref())?;

        // set ALPN protocols
        static PROTOS: &[u8] = b"\x02h2\x08http/1.1";
        builder.set_alpn_protos(PROTOS)?;
        // set uo ALPN selection routine - as select_next_proto
        builder.set_alpn_select_callback(move |_: &mut SslRef, list: &[u8]| {
            openssl::ssl::select_next_proto(PROTOS, list).ok_or(openssl::ssl::AlpnError::NOACK)
        });
        Ok(builder)
    }
}

pin_project! {
    /// OpensslListener
    pub struct OpensslListener<C> {
        #[pin]
        config_stream: C,
        incoming: AddrIncoming,
        openssl_config: Option<OpensslConfig>,
        acceptor: Option<Arc<SslAcceptor>>,
    }
}
/// OpensslListener
pub struct OpensslListenerBuilder<C> {
    config_stream: C,
}
impl<C> OpensslListenerBuilder<C>
where
    C: Stream,
    C::Item: Into<OpensslConfig>,
{
    /// Bind to socket address.
    #[inline]
    pub fn bind(self, incoming: impl IntoAddrIncoming) -> OpensslListener<C> {
        self.try_bind(incoming).unwrap()
    }
    /// Try to bind to socket address.
    #[inline]
    pub fn try_bind(self, incoming: impl IntoAddrIncoming) -> Result<OpensslListener<C>, hyper::Error> {
        Ok(OpensslListener {
            config_stream: self.config_stream,
            incoming: incoming.into_incoming(),
            openssl_config: None,
            acceptor: None,
        })
    }
}

impl<C> OpensslListener<C> {
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
impl OpensslListener<stream::Once<Ready<OpensslConfig>>> {
    /// Create new OpensslListenerBuilder with OpensslConfig.
    #[inline]
    pub fn with_openssl_config(
        config: OpensslConfig,
    ) -> OpensslListenerBuilder<stream::Once<Ready<OpensslConfig>>> {
        Self::try_with_openssl_config(config).unwrap()
    }
    /// Try to create new OpensslListenerBuilder with OpensslConfig.
    #[inline]
    pub fn try_with_openssl_config(
        config: OpensslConfig,
    ) -> Result<OpensslListenerBuilder<stream::Once<Ready<OpensslConfig>>>, IoError> {
        let stream = futures_util::stream::once(futures_util::future::ready(config.into()));
        Ok(Self::with_config_stream(stream))
    }
}

impl<C> OpensslListener<C>
where
    C: Stream,
    C::Item: Into<OpensslConfig>,
{
    /// Create new OpensslListener with config stream.
    #[inline]
    pub fn with_config_stream(config_stream: C) -> OpensslListenerBuilder<C> {
        OpensslListenerBuilder { config_stream }
    }
}

impl<C> Listener for OpensslListener<C>
where
    C: Stream,
    C::Item: Into<OpensslConfig>,
{
}
impl<C> Accept for OpensslListener<C>
where
    C: Stream,
    C::Item: Into<OpensslConfig>,
{
    type Conn = OpensslStream;
    type Error = IoError;

    #[inline]
    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let this = self.project();
        if let Poll::Ready(Some(config)) = this.config_stream.poll_next(cx) {
            let config: OpensslConfig = config.into();
            let builder = config.create_acceptor_builder()?;
            *this.acceptor = Some(Arc::new(builder.build()));
        }
        if let Some(acceptor) = &this.acceptor {
            match ready!(Pin::new(this.incoming).poll_accept(cx)) {
                Some(Ok(sock)) => {
                    let remote_addr = sock.remote_addr();
                    let ssl =
                        Ssl::new(acceptor.context()).map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
                    let stream =
                        SslStream::new(ssl, sock).map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
                    Poll::Ready(Some(Ok(OpensslStream::new(remote_addr.into(), stream))))
                }
                Some(Err(e)) => Poll::Ready(Some(Err(e))),
                None => Poll::Ready(None),
            }
        } else {
            Poll::Ready(Some(Err(IoError::new(
                ErrorKind::Other,
                "faild to load openssl server config",
            ))))
        }
    }
}

/// tokio_openssl::server::TlsStream doesn't expose constructor methods,
/// so we have to TlsAcceptor::accept and handshake to have access to it
/// OpensslStream implements AsyncRead/AsyncWrite handshaking tokio_openssl::Accept first
pub struct OpensslStream {
    inner_stream: SslStream<AddrStream>,
    remote_addr: SocketAddr,
}
impl Transport for OpensslStream {
    #[inline]
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr.clone())
    }
}

impl OpensslStream {
    #[inline]
    fn new(remote_addr: SocketAddr, inner_stream: SslStream<AddrStream>) -> Self {
        OpensslStream {
            remote_addr,
            inner_stream,
        }
    }
}

impl AsyncRead for OpensslStream {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        Pin::new(&mut pin.inner_stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for OpensslStream {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let pin = self.get_mut();
        Pin::new(&mut pin.inner_stream).poll_write(cx, buf)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        Pin::new(&mut pin.inner_stream).poll_flush(cx)
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        Pin::new(&mut pin.inner_stream).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use futures_util::{Stream, StreamExt};
    use openssl::ssl::SslConnector;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

    impl<C> Stream for OpensslListener<C>
    where
        C: Stream,
        C::Item: Into<OpensslConfig>,
    {
        type Item = Result<OpensslStream, IoError>;
        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.poll_accept(cx)
        }
    }

    #[tokio::test]
    async fn test_openssl_listener() {
        let mut listener = OpensslListener::with_openssl_config(
            OpensslConfig::new()
                .with_key_path("certs/cert.pem")
                .with_cert_path("certs/key.pem"),
        )
        .bind("127.0.0.1:0");
        let addr = listener.local_addr();

        tokio::spawn(async move {
            let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
            connector.set_ca_file("certs/chain.pem").unwrap();

            let ssl = connector
                .build()
                .configure()
                .unwrap()
                .into_ssl("testserver.com")
                .unwrap();

            let stream = TcpStream::connect(addr).await.unwrap();
            let mut tls_stream = SslStream::new(ssl, stream).unwrap();
            Pin::new(&mut tls_stream).connect().await.unwrap();
            tls_stream.write_i32(518).await.unwrap();
        });

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 518);
    }
}
