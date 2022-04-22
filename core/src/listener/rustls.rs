//! tls module
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
use pin_project_lite::pin_project;
use rustls_pemfile::{self, pkcs8_private_keys, rsa_private_keys};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, ErrorKind, ReadBuf};
pub use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::rustls::server::{AllowAnyAnonymousOrAuthenticatedClient, AllowAnyAuthenticatedClient, NoClientAuth};
use tokio_rustls::rustls::{Certificate, Error as RustlsError, PrivateKey, RootCertStore};

use super::{IntoAddrIncoming, LazyFile, Listener};
use crate::addr::SocketAddr;
use crate::transport::Transport;

/// Represents errors that can occur building the RustlsListener
#[cfg_attr(docsrs, doc(cfg(feature = "rustls")))]
#[derive(Debug, Error)]
pub enum Error {
    /// Hyper error
    #[error("hyper error")]
    Hyper(hyper::Error),
    /// An IO error
    #[error("io error")]
    Io(IoError),
    /// An Error parsing the Certificate
    #[error("certificate parse error")]
    CertParseError,
    /// An Error parsing a Pkcs8 key
    #[error("pkcs8 parse error")]
    Pkcs8ParseError,
    /// An Error parsing a Rsa key
    #[error("rsa parse error")]
    RsaParseError,
    /// An error from an empty key
    #[error("key contains no private key")]
    EmptyKey,
    /// An error from an invalid key
    #[error("key contains an invalid key, {0}")]
    InvalidKey(RustlsError),
}

/// Tls client authentication configuration.
pub(crate) enum TlsClientAuth {
    /// No client auth.
    Off,
    /// Allow any anonymous or authenticated client.
    Optional(Box<dyn Read + Send + Sync>),
    /// Allow any authenticated client.
    Required(Box<dyn Read + Send + Sync>),
}

/// Builder to set the configuration for the Tls server.
#[cfg_attr(docsrs, doc(cfg(feature = "rustls")))]
pub struct RustlsConfig {
    cert: Box<dyn Read + Send + Sync>,
    key: Box<dyn Read + Send + Sync>,
    client_auth: TlsClientAuth,
    ocsp_resp: Vec<u8>,
}

impl fmt::Debug for RustlsConfig {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("RustlsConfig").finish()
    }
}
impl Default for RustlsConfig {
    fn default() -> Self {
        RustlsConfig::new()
    }
}

impl RustlsConfig {
    /// Create new `RustlsConfig`
    #[inline]
    pub fn new() -> Self {
        RustlsConfig {
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

    /// Sets the trust anchor for optional Tls client authentication via file path.
    ///
    /// Anonymous and authenticated clients will be accepted. If no trust anchor is provided by any
    /// of the `client_auth_` methods, then client authentication is disabled by default.
    #[inline]
    pub fn with_client_auth_optional_path(mut self, path: impl AsRef<Path>) -> Self {
        let file = Box::new(LazyFile {
            path: path.as_ref().into(),
            file: None,
        });
        self.client_auth = TlsClientAuth::Optional(file);
        self
    }

    /// Sets the trust anchor for optional Tls client authentication via bytes slice.
    ///
    /// Anonymous and authenticated clients will be accepted. If no trust anchor is provided by any
    /// of the `client_auth_` methods, then client authentication is disabled by default.
    pub fn with_client_auth_optional(mut self, trust_anchor: impl Into<Vec<u8>>) -> Self {
        let cursor = Box::new(Cursor::new(trust_anchor.into()));
        self.client_auth = TlsClientAuth::Optional(cursor);
        self
    }

    /// Sets the trust anchor for required Tls client authentication via file path.
    ///
    /// Only authenticated clients will be accepted. If no trust anchor is provided by any of the
    /// `client_auth_` methods, then client authentication is disabled by default.
    #[inline]
    pub fn with_client_auth_required_path(mut self, path: impl AsRef<Path>) -> Self {
        let file = Box::new(LazyFile {
            path: path.as_ref().into(),
            file: None,
        });
        self.client_auth = TlsClientAuth::Required(file);
        self
    }

    /// Sets the trust anchor for required Tls client authentication via bytes slice.
    ///
    /// Only authenticated clients will be accepted. If no trust anchor is provided by any of the
    /// `client_auth_` methods, then client authentication is disabled by default.
    #[inline]
    pub fn with_client_auth_required(mut self, trust_anchor: impl Into<Vec<u8>>) -> Self {
        let cursor = Box::new(Cursor::new(trust_anchor.into()));
        self.client_auth = TlsClientAuth::Required(cursor);
        self
    }

    /// Sets the DER-encoded OCSP response
    #[inline]
    pub fn with_ocsp_resp(mut self, ocsp_resp: impl Into<Vec<u8>>) -> Self {
        self.ocsp_resp = ocsp_resp.into();
        self
    }
    /// ServerConfig
    pub fn build_server_config(mut self) -> Result<ServerConfig, Error> {
        let mut cert_rdr = BufReader::new(self.cert);
        let cert_chain = rustls_pemfile::certs(&mut cert_rdr)
            .map_err(|_| Error::CertParseError)?
            .into_iter()
            .map(Certificate)
            .collect();

        let key = {
            // convert it to Vec<u8> to allow reading it again if key is RSA
            let mut key_vec = Vec::new();
            self.key.read_to_end(&mut key_vec).map_err(Error::Io)?;

            if key_vec.is_empty() {
                return Err(Error::EmptyKey);
            }

            let mut pkcs8 = pkcs8_private_keys(&mut key_vec.as_slice()).map_err(|_| Error::Pkcs8ParseError)?;

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
        };

        let client_auth = match self.client_auth {
            TlsClientAuth::Off => NoClientAuth::new(),
            TlsClientAuth::Optional(trust_anchor) => {
                AllowAnyAnonymousOrAuthenticatedClient::new(read_trust_anchor(trust_anchor)?)
            }
            TlsClientAuth::Required(trust_anchor) => AllowAnyAuthenticatedClient::new(read_trust_anchor(trust_anchor)?),
        };

        let config = ServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_safe_default_protocol_versions()
            .map_err(|_| Error::RsaParseError)?
            .with_client_cert_verifier(client_auth)
            .with_single_cert_with_ocsp_and_sct(cert_chain, PrivateKey(key), self.ocsp_resp, Vec::new())
            .map_err(Error::InvalidKey)?;
        Ok(config)
    }
}

fn read_trust_anchor(trust_anchor: Box<dyn Read + Send + Sync>) -> Result<RootCertStore, Error> {
    let mut reader = BufReader::new(trust_anchor);
    let certs = rustls_pemfile::certs(&mut reader).map_err(|_| Error::RsaParseError)?;
    let mut store = RootCertStore::empty();
    if let (0, _) = store.add_parsable_certificates(&certs) {
        Err(Error::CertParseError)
    } else {
        Ok(store)
    }
}

pin_project! {
    /// RustlsListener
    #[cfg_attr(docsrs, doc(cfg(feature = "rustls")))]
    pub struct RustlsListener<C> {
        #[pin]
        config_stream: C,
        incoming: AddrIncoming,
        server_config: Option<Arc<ServerConfig>>,
    }
}
/// RustlsListener
#[cfg_attr(docsrs, doc(cfg(feature = "rustls")))]
pub struct RustlsListenerBuilder<C> {
    config_stream: C,
}
impl<C> RustlsListenerBuilder<C>
where
    C: Stream,
    C::Item: Into<Arc<ServerConfig>>,
{
    /// Bind to socket address.
    #[inline]
    pub fn bind(self, incoming: impl IntoAddrIncoming) -> RustlsListener<C> {
        self.try_bind(incoming).unwrap()
    }
    /// Try to bind to socket address.
    #[inline]
    pub fn try_bind(self, incoming: impl IntoAddrIncoming) -> Result<RustlsListener<C>, hyper::Error> {
        Ok(RustlsListener {
            config_stream: self.config_stream,
            incoming: incoming.into_incoming(),
            server_config: None,
        })
    }
}

impl<C> RustlsListener<C> {
    /// Get local address
    pub fn local_addr(&self) -> SocketAddr {
        self.incoming.local_addr().into()
    }
}
impl RustlsListener<stream::Once<Ready<Arc<ServerConfig>>>> {
    /// Create new RustlsListenerBuilder with RustlsConfig.
    #[inline]
    pub fn with_rustls_config(config: RustlsConfig) -> RustlsListenerBuilder<stream::Once<Ready<Arc<ServerConfig>>>> {
        Self::try_with_rustls_config(config).unwrap()
    }
    /// Try to create new RustlsListenerBuilder with RustlsConfig.
    #[inline]
    pub fn try_with_rustls_config(
        config: RustlsConfig,
    ) -> Result<RustlsListenerBuilder<stream::Once<Ready<Arc<ServerConfig>>>>, Error> {
        let config = config.build_server_config()?;
        let stream = futures_util::stream::once(futures_util::future::ready(config.into()));
        Ok(Self::with_config_stream(stream))
    }
    /// Create new RustlsListenerBuilder with ServerConfig.
    #[inline]
    pub fn with_server_config(
        config: impl Into<Arc<ServerConfig>>,
    ) -> RustlsListenerBuilder<stream::Once<Ready<Arc<ServerConfig>>>> {
        let stream = futures_util::stream::once(futures_util::future::ready(config.into()));
        Self::with_config_stream(stream)
    }
}

impl From<RustlsConfig> for Arc<ServerConfig> {
    fn from(rustls_config: RustlsConfig) -> Self {
        rustls_config.build_server_config().unwrap().into()
    }
}

impl<C> RustlsListener<C>
where
    C: Stream,
    C::Item: Into<Arc<ServerConfig>>,
{
    /// Create new RustlsListener with config stream.
    #[inline]
    pub fn with_config_stream(config_stream: C) -> RustlsListenerBuilder<C> {
        RustlsListenerBuilder { config_stream }
    }
}

impl<C> Listener for RustlsListener<C>
where
    C: Stream,
    C::Item: Into<Arc<ServerConfig>>,
{
}
impl<C> Accept for RustlsListener<C>
where
    C: Stream,
    C::Item: Into<Arc<ServerConfig>>,
{
    type Conn = RustlsStream;
    type Error = IoError;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let this = self.project();
        if let Poll::Ready(Some(config)) = this.config_stream.poll_next(cx) {
            *this.server_config = Some(config.into());
        }
        if let Some(server_config) = &this.server_config {
            match ready!(Pin::new(this.incoming).poll_accept(cx)) {
                Some(Ok(sock)) => Poll::Ready(Some(Ok(RustlsStream::new(sock, server_config.clone())))),
                Some(Err(e)) => Poll::Ready(Some(Err(e))),
                None => Poll::Ready(None),
            }
        } else {
            Poll::Ready(Some(Err(IoError::new(
                ErrorKind::Other,
                "faild to load rustls server config",
            ))))
        }
    }
}

enum RustlsState {
    Handshaking(tokio_rustls::Accept<AddrStream>),
    Streaming(tokio_rustls::server::TlsStream<AddrStream>),
}

/// tokio_rustls::server::TlsStream doesn't expose constructor methods,
/// so we have to TlsAcceptor::accept and handshake to have access to it
/// RustlsStream implements AsyncRead/AsyncWrite handshaking tokio_rustls::Accept first
#[cfg_attr(docsrs, doc(cfg(feature = "rustls")))]
pub struct RustlsStream {
    state: RustlsState,
    remote_addr: SocketAddr,
}
impl Transport for RustlsStream {
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr.clone())
    }
}

impl RustlsStream {
    fn new(stream: AddrStream, config: Arc<ServerConfig>) -> Self {
        let remote_addr = stream.remote_addr();
        let accept = tokio_rustls::TlsAcceptor::from(config).accept(stream);
        RustlsStream {
            state: RustlsState::Handshaking(accept),
            remote_addr: remote_addr.into(),
        }
    }
}

impl AsyncRead for RustlsStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        match pin.state {
            RustlsState::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_read(cx, buf);
                    pin.state = RustlsState::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            RustlsState::Streaming(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for RustlsStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let pin = self.get_mut();
        match pin.state {
            RustlsState::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_write(cx, buf);
                    pin.state = RustlsState::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            RustlsState::Streaming(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            RustlsState::Handshaking(_) => Poll::Ready(Ok(())),
            RustlsState::Streaming(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            RustlsState::Handshaking(_) => Poll::Ready(Ok(())),
            RustlsState::Streaming(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio_rustls::rustls::{ClientConfig, ServerName};
    use tokio_rustls::TlsConnector;

    use super::*;

    impl<C> Stream for RustlsListener<C>
    where
        C: Stream,
        C::Item: Into<Arc<ServerConfig>>,
    {
        type Item = Result<RustlsStream, IoError>;
        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.poll_accept(cx)
        }
    }
    #[test]
    fn test_file_cert_key() {
        RustlsConfig::new()
            .with_key_path("certs/end.rsa")
            .with_cert_path("certs/end.cert")
            .build_server_config()
            .unwrap();
    }

    #[tokio::test]
    async fn test_rustls_listener() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 7978));
        let mut listener = RustlsListener::with_rustls_config(
            RustlsConfig::new()
                .with_key_path("certs/end.rsa")
                .with_cert_path("certs/end.cert"),
        )
        .bind(addr);

        tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            let trust_anchor = include_bytes!("../../certs/end.chain");
            let client_config = ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(read_trust_anchor(Box::new(trust_anchor.as_slice())).unwrap())
                .with_no_client_auth();
            let connector = TlsConnector::from(Arc::new(client_config));
            let mut tls_stream = connector
                .connect(ServerName::try_from("testserver.com").unwrap(), stream)
                .await
                .unwrap();
            tls_stream.write_i32(518).await.unwrap();
        });

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 518);
    }
}
