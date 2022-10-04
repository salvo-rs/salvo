//! openssl module
use std::fmt::{self, Formatter};
use std::fs::File;
use std::io::{self, Error as IoError, Read};
use std::path::{Path, PathBuf};
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
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ErrorKind, ReadBuf};
use tokio_openssl::SslStream;

use super::{IntoAddrIncoming, Listener};
use crate::addr::SocketAddr;
use crate::transport::Transport;

/// Private key and certificate
#[derive(Debug)]
pub struct Keycert {
    key_path: Option<PathBuf>,
    key: Vec<u8>,
    cert_path: Option<PathBuf>,
    cert: Vec<u8>,
}

impl Default for Keycert {
    fn default() -> Self {
        Self::new()
    }
}

impl Keycert {
    /// Create a new keycert.
    #[inline]
    pub fn new() -> Self {
        Self {
            key_path: None,
            key: vec![],
            cert_path: None,
            cert: vec![],
        }
    }
    /// Sets the Tls private key via File Path, returns `Error::IoError` if the file cannot be open.
    #[inline]
    pub fn with_key_path(mut self, path: impl AsRef<Path>) -> Self {
        self.key_path = Some(path.as_ref().into());
        self
    }

    /// Sets the Tls private key via bytes slice.
    #[inline]
    pub fn with_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.key = key.into();
        self
    }

    /// Specify the file path for the TLS certificate to use.
    #[inline]
    pub fn with_cert_path(mut self, path: impl AsRef<Path>) -> Self {
        self.cert_path = Some(path.as_ref().into());
        self
    }

    /// Sets the Tls certificate via bytes slice
    #[inline]
    pub fn with_cert(mut self, cert: impl Into<Vec<u8>>) -> Self {
        self.cert = cert.into();
        self
    }

    /// Get the private key.
    #[inline]
    pub fn key(&mut self) -> io::Result<&[u8]> {
        if self.key.is_empty() {
            if let Some(path) = &self.key_path {
                let mut file = File::open(path)?;
                file.read_to_end(&mut self.key)?;
            }
        }
        if self.key.is_empty() {
            Err(IoError::new(ErrorKind::Other, "empty key"))
        } else {
            Ok(&self.key)
        }
    }

    /// Get the cert.
    #[inline]
    pub fn cert(&mut self) -> io::Result<&[u8]> {
        if self.cert.is_empty() {
            if let Some(path) = &self.cert_path {
                let mut file = File::open(path)?;
                file.read_to_end(&mut self.cert)?;
            }
        }
        if self.cert.is_empty() {
            Err(IoError::new(ErrorKind::Other, "empty cert"))
        } else {
            Ok(&self.cert)
        }
    }
}

/// Builder to set the configuration for the Tls server.
pub struct OpensslConfig {
    keycert: Keycert,
}

impl fmt::Debug for OpensslConfig {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("OpensslConfig").finish()
    }
}

impl OpensslConfig {
    /// Create new `OpensslConfig`
    #[inline]
    pub fn new(keycert: Keycert) -> Self {
        OpensslConfig { keycert }
    }

    /// Create [`SslAcceptorBuilder`]
    pub fn create_acceptor_builder(mut self) -> Result<SslAcceptorBuilder, IoError> {
        let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;

        let mut certs = X509::stack_from_pem(self.keycert.cert()?)?;
        let mut certs = certs.drain(..);
        builder.set_certificate(
            certs
                .next()
                .ok_or_else(|| IoError::new(ErrorKind::Other, "no leaf certificate"))?
                .as_ref(),
        )?;
        certs.try_for_each(|cert| builder.add_extra_chain_cert(cert))?;
        builder.set_private_key(PKey::private_key_from_pem(self.keycert.key()?)?.as_ref())?;

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

/// OpensslListener
#[pin_project]
pub struct OpensslListener<C> {
    #[pin]
    config_stream: C,
    incoming: AddrIncoming,
    openssl_config: Option<OpensslConfig>,
    acceptor: Option<Arc<SslAcceptor>>,
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
    pub fn with_config(config: OpensslConfig) -> OpensslListenerBuilder<stream::Once<Ready<OpensslConfig>>> {
        Self::try_with_config(config).unwrap()
    }
    /// Try to create new OpensslListenerBuilder with OpensslConfig.
    #[inline]
    pub fn try_with_config(
        config: OpensslConfig,
    ) -> Result<OpensslListenerBuilder<stream::Once<Ready<OpensslConfig>>>, IoError> {
        let stream = futures_util::stream::once(futures_util::future::ready(config));
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
                "failed to load openssl server config",
            ))))
        }
    }
}

/// OpensslStream implements AsyncRead/AsyncWrite handshaking tokio_openssl::Accept first
pub struct OpensslStream {
    inner_stream: SslStream<AddrStream>,
    remote_addr: SocketAddr,
    is_ready: bool,
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
            is_ready: false,
        }
    }
    #[inline]
    fn sync_ready(&mut self, cx: &mut Context) -> io::Result<bool> {
        if !self.is_ready {
            let result = Pin::new(&mut self.inner_stream)
                .poll_accept(cx)
                .map_err(|_| IoError::new(ErrorKind::Other, "failed to accept in openssl"))?;
            if result.is_ready() {
                self.is_ready = true;
            }
        }
        Ok(self.is_ready)
    }
}

impl AsyncRead for OpensslStream {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        if pin.sync_ready(cx)? {
            Pin::new(&mut pin.inner_stream).poll_read(cx, buf)
        } else {
            Poll::Pending
        }
    }
}

impl AsyncWrite for OpensslStream {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let pin = self.get_mut();
        if pin.sync_ready(cx)? {
            Pin::new(&mut pin.inner_stream).poll_write(cx, buf)
        } else {
            Poll::Pending
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        if pin.sync_ready(cx)? {
            Pin::new(&mut pin.inner_stream).poll_flush(cx)
        } else {
            Poll::Pending
        }
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner_stream).poll_shutdown(cx)
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
        let config = OpensslConfig::new(
            Keycert::new()
                .with_key_path("certs/key.pem")
                .with_cert_path("certs/cert.pem"),
        );
        let mut listener = OpensslListener::with_config(config).bind("127.0.0.1:0");
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
