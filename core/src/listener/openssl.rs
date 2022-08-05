//! openssl module
use std::fmt::{self, Formatter};
use std::future::Future;
use std::io::{self, BufReader, Cursor, Error as IoError, Read};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::Ready;
use futures_util::{ready, stream, Stream};
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use openssl::{
    pkey::PKey,
    ssl::{Ssl, SslAcceptor, SslAcceptorBuilder, SslFiletype, SslMethod, SslRef},
    x509::X509,
};
use pin_project_lite::pin_project;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, ErrorKind, ReadBuf};

use super::{IntoAddrIncoming, LazyFile, Listener};
use crate::addr::SocketAddr;
use crate::transport::Transport;

/// Represents errors that can occur building the OpensslListener
#[derive(Debug, Error)]
pub enum Error {
    /// Hyper error
    #[error("Hyper error: {0}")]
    Hyper(hyper::Error),
    /// An IO error
    #[error("I/O error: {0}")]
    Io(IoError),
    /// An Error parsing the Certificate
    #[error("Certificate parse error")]
    CertParseError,
    /// An Error parsing a Pkcs8 key
    #[error("Pkcs8 parse error")]
    Pkcs8ParseError,
    /// An Error parsing a Rsa key
    #[error("Rsa parse error")]
    RsaParseError,
    /// An error from an empty key
    #[error("Empy key")]
    EmptyKey,
    /// An error from an invalid key
    #[error("Invalid key, {0}")]
    InvalidKey(OpensslError),
}

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
            client_auth: TlsClientAuth::Off,
            ocsp_resp: Vec::new(),
        }
    }

    /// sets the Tls key via File Path, returns `Error::IoError` if the file cannot be open
    #[inline]
    pub fn with_key_path(mut self, path: impl AsRef<Path>) -> Self {
        self.key = Box::new(LazyFile {
            path: path.as_ref().into(),
            file: None,
        });
        self
    }

    /// sets the Tls key via bytes slice
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

    /// sets the Tls certificate via bytes slice
    #[inline]
    pub fn with_cert(mut self, cert: impl Into<Vec<u8>>) -> Self {
        self.cert = Box::new(Cursor::new(cert.into()));
        self
    }

    /// Create [`SslAcceptorBuilder`]
    pub fn create_acceptor_builder(mut self) -> Result<SslAcceptorBuilder, Error> {
        let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;

        let mut cert_vec = Vec::new();
        self.cert.read_to_end(&mut key_vec).map_err(Error::Io)?;
        if cert_vec.is_empty() {
            return Err(Error::EmptyCert);
        }
        let mut certs = X509::stack_from_pem(cert_vec)?;
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
        self.key.read_to_end(&mut key_vec).map_err(Error::Io)?;

        if key_vec.is_empty() {
            return Err(Error::EmptyKey);
        }

        builder.set_private_key(PKey::private_key_from_pem(data)?.as_ref())?;

        if !pkcs8.is_empty() {
            pkcs8.remove(0)
        } else {
            let mut rsa = rsa_private_keys(&mut key_vec.as_slice()).map_err(|_| Error::RsaParseError)?;

            if !rsa.is_empty() {
                rsa.remove(0)
            } else {
                return Err(Error::EmptyKey);
            }
        }

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
        server_config: Option<Arc<ServerConfig>>,
    }
}
/// OpensslListener
pub struct OpensslListenerBuilder<C> {
    config_stream: C,
}
impl<C> OpensslListenerBuilder<C>
where
    C: Stream,
    C::Item: Into<Arc<ServerConfig>>,
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
            server_config: None,
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
impl OpensslListener<stream::Once<Ready<Arc<ServerConfig>>>> {
    /// Create new OpensslListenerBuilder with OpensslConfig.
    #[inline]
    pub fn with_openssl_config(
        config: OpensslConfig,
    ) -> OpensslListenerBuilder<stream::Once<Ready<Arc<ServerConfig>>>> {
        Self::try_with_openssl_config(config).unwrap()
    }
    /// Try to create new OpensslListenerBuilder with OpensslConfig.
    #[inline]
    pub fn try_with_openssl_config(
        config: OpensslConfig,
    ) -> Result<OpensslListenerBuilder<stream::Once<Ready<Arc<ServerConfig>>>>, Error> {
        let config = config.build_server_config()?;
        let stream = futures_util::stream::once(futures_util::future::ready(config.into()));
        Ok(Self::with_config_stream(stream))
    }
    /// Create new OpensslListenerBuilder with ServerConfig.
    #[inline]
    pub fn with_server_config(
        config: impl Into<Arc<ServerConfig>>,
    ) -> OpensslListenerBuilder<stream::Once<Ready<Arc<ServerConfig>>>> {
        let stream = futures_util::stream::once(futures_util::future::ready(config.into()));
        Self::with_config_stream(stream)
    }
}

impl From<OpensslConfig> for Arc<ServerConfig> {
    #[inline]
    fn from(openssl_config: OpensslConfig) -> Self {
        openssl_config.build_server_config().unwrap().into()
    }
}

impl<C> OpensslListener<C>
where
    C: Stream,
    C::Item: Into<Arc<ServerConfig>>,
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
    C::Item: Into<Arc<ServerConfig>>,
{
}
impl<C> Accept for OpensslListener<C>
where
    C: Stream,
    C::Item: Into<Arc<ServerConfig>>,
{
    type Conn = OpensslStream;
    type Error = IoError;

    #[inline]
    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let this = self.project();
        if let Poll::Ready(Some(config)) = this.config_stream.poll_next(cx) {
            *this.server_config = Some(config.into());
        }
        if let Some(server_config) = &this.server_config {
            match ready!(Pin::new(this.incoming).poll_accept(cx)) {
                Some(Ok(sock)) => Poll::Ready(Some(Ok(OpensslStream::new(sock, server_config.clone())))),
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

enum OpensslState {
    Handshaking(tokio_openssl::Accept<AddrStream>),
    Streaming(tokio_openssl::server::TlsStream<AddrStream>),
}

/// tokio_openssl::server::TlsStream doesn't expose constructor methods,
/// so we have to TlsAcceptor::accept and handshake to have access to it
/// OpensslStream implements AsyncRead/AsyncWrite handshaking tokio_openssl::Accept first
pub struct OpensslStream {
    state: OpensslState,
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
    fn new(stream: AddrStream, config: Arc<ServerConfig>) -> Self {
        let remote_addr = stream.remote_addr();
        let accept = tokio_openssl::TlsAcceptor::from(config).accept(stream);
        OpensslStream {
            state: OpensslState::Handshaking(accept),
            remote_addr: remote_addr.into(),
        }
    }
}

impl AsyncRead for OpensslStream {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        match pin.state {
            OpensslState::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_read(cx, buf);
                    pin.state = OpensslState::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            OpensslState::Streaming(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for OpensslStream {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let pin = self.get_mut();
        match pin.state {
            OpensslState::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_write(cx, buf);
                    pin.state = OpensslState::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            OpensslState::Streaming(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            OpensslState::Handshaking(_) => Poll::Ready(Ok(())),
            OpensslState::Streaming(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    #[inline]
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            OpensslState::Handshaking(_) => Poll::Ready(Ok(())),
            OpensslState::Streaming(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio_openssl::openssl::{ClientConfig, ServerName};
    use tokio_openssl::TlsConnector;

    use super::*;

    impl<C> Stream for OpensslListener<C>
    where
        C: Stream,
        C::Item: Into<Arc<ServerConfig>>,
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
                .with_key_path("certs/rsa/cert.pem")
                .with_cert_path("certs/rsa/key.pem"),
        )
        .bind("127.0.0.1:0");
        let addr = listener.local_addr();

        tokio::spawn(async move {
            let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
            connector.set_ca_file("src/listener/certs/chain1.pem").unwrap();

            let ssl = connector
                .build()
                .configure()
                .unwrap()
                .into_ssl("testserver.com")
                .unwrap();

            let stream = TcpStream::connect(local_addr.as_socket_addr().unwrap()).await.unwrap();
            let mut tls_stream = SslStream::new(ssl, stream).unwrap();
            use std::pin::Pin;
            Pin::new(&mut tls_stream).connect().await.unwrap();

            tls_stream.write_i32(518).await.unwrap();
        });

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 518);
    }
}
