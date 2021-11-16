//! tls module
use std::fs::File;
use std::future::Future;
use std::io::{self, BufReader, Cursor, Read};
use std::net::SocketAddr as StdSocketAddr;
use std::path::{Path, PathBuf};
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
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
pub use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::rustls::server::{AllowAnyAnonymousOrAuthenticatedClient, AllowAnyAuthenticatedClient, NoClientAuth};
use tokio_rustls::rustls::{Certificate, Error as RustlsError, PrivateKey, RootCertStore};

use super::Listener;
use crate::addr::SocketAddr;
use crate::transport::Transport;

/// Represents errors that can occur building the RustlsListener
#[derive(Debug, Error)]
pub enum Error {
    /// Hyper error
    #[error("hyper error")]
    Hyper(hyper::Error),
    /// An IO error
    #[error("io error")]
    Io(io::Error),
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
pub struct RustlsConfig {
    cert: Box<dyn Read + Send + Sync>,
    key: Box<dyn Read + Send + Sync>,
    client_auth: TlsClientAuth,
    ocsp_resp: Vec<u8>,
}

impl std::fmt::Debug for RustlsConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_struct("RustlsConfig").finish()
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

pin_project! {
    /// RustlsListener
    pub struct RustlsListener<C> {
        #[pin]
        config_stream: C,
        incoming: AddrIncoming,
        server_config: Option<Arc<ServerConfig>>,
    }
}
/// RustlsListener
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
            incoming: incoming.into(),
            server_config: None,
        })
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
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let this = self.project();
        if let Poll::Ready(result) = this.config_stream.poll_next(cx) {
            if let Some(config) = result {
                *this.server_config = Some(config.into());
            }
        }
        match this.server_config.clone() {
            Some(server_config) => match ready!(Pin::new(this.incoming).poll_accept(cx)) {
                Some(Ok(sock)) => Poll::Ready(Some(Ok(RustlsStream::new(sock, server_config)))),
                Some(Err(e)) => Poll::Ready(Some(Err(e))),
                None => Poll::Ready(None),
            },
            None => Poll::Ready(Some(Err(io::Error::new(
                io::ErrorKind::Other,
                "faild to load rustls server config",
            )))),
        }
    }
}

struct LazyFile {
    path: PathBuf,
    file: Option<File>,
}

impl LazyFile {
    fn lazy_read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.file.is_none() {
            self.file = Some(File::open(&self.path)?);
        }

        self.file.as_mut().unwrap().read(buf)
    }
}

impl Read for LazyFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.lazy_read(buf).map_err(|err| {
            let kind = err.kind();
            tracing::error!(path = ?self.path, error = ?err, "error reading file");
            io::Error::new(kind, format!("error reading file ({:?}): {}", self.path.display(), err))
        })
    }
}

enum State {
    Handshaking(tokio_rustls::Accept<AddrStream>),
    Streaming(tokio_rustls::server::TlsStream<AddrStream>),
}

/// tokio_rustls::server::TlsStream doesn't expose constructor methods,
/// so we have to TlsAcceptor::accept and handshake to have access to it
/// RustlsStream implements AsyncRead/AsyncWrite handshaking tokio_rustls::Accept first
pub struct RustlsStream {
    state: State,
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
            state: State::Handshaking(accept),
            remote_addr: remote_addr.into(),
        }
    }
}

impl AsyncRead for RustlsStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        match pin.state {
            State::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_read(cx, buf);
                    pin.state = State::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            State::Streaming(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for RustlsStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let pin = self.get_mut();
        match pin.state {
            State::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_write(cx, buf);
                    pin.state = State::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            State::Streaming(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            State::Handshaking(_) => Poll::Ready(Ok(())),
            State::Streaming(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            State::Handshaking(_) => Poll::Ready(Ok(())),
            State::Streaming(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_cert_key() {
        RustlsConfig::new()
            .with_key_path("../examples/tls/key.rsa")
            .with_cert_path("../examples/tls/cert.pem")
            .build_server_config()
            .unwrap();
    }

    #[test]
    fn bytes_cert_key() {
        let key = include_str!("../../../examples/tls/key.rsa");
        let cert = include_str!("../../../examples/tls/cert.pem");

        RustlsConfig::new()
            .with_key(key.as_bytes())
            .with_cert(cert.as_bytes())
            .build_server_config()
            .unwrap();
    }
}
