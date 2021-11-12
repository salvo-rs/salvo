//! tls module
use std::fs::File;
use std::future::Future;
use std::io::{self, BufReader, Cursor, Read};
use std::net::SocketAddr as StdSocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::ready;
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use rustls_pemfile::{self, pkcs8_private_keys, rsa_private_keys};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::rustls::server::{
    AllowAnyAnonymousOrAuthenticatedClient, AllowAnyAuthenticatedClient, NoClientAuth, ServerConfig,
};
use tokio_rustls::rustls::{Certificate, Error as RustlsError, PrivateKey, RootCertStore};

use super::Listener;
use crate::addr::SocketAddr;
use crate::transport::Transport;

/// Represents errors that can occur building the TlsListener
#[derive(Debug, Error)]
pub enum TlsError {
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
pub struct TlsListenerBuilder {
    cert: Box<dyn Read + Send + Sync>,
    key: Box<dyn Read + Send + Sync>,
    client_auth: TlsClientAuth,
    ocsp_resp: Vec<u8>,
}

impl std::fmt::Debug for TlsListenerBuilder {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_struct("TlsListenerBuilder").finish()
    }
}

impl TlsListenerBuilder {
    /// Create a new TlsListenerBuilder
    pub fn new() -> Self {
        TlsListenerBuilder {
            key: Box::new(io::empty()),
            cert: Box::new(io::empty()),
            client_auth: TlsClientAuth::Off,
            ocsp_resp: Vec::new(),
        }
    }

    /// sets the Tls key via File Path, returns `TlsError::IoError` if the file cannot be open
    pub fn with_key_path(mut self, path: impl AsRef<Path>) -> Self {
        self.key = Box::new(LazyFile {
            path: path.as_ref().into(),
            file: None,
        });
        self
    }

    /// sets the Tls key via bytes slice
    pub fn with_key(mut self, key: &[u8]) -> Self {
        self.key = Box::new(Cursor::new(Vec::from(key)));
        self
    }

    /// Specify the file path for the TLS certificate to use.
    pub fn with_cert_path(mut self, path: impl AsRef<Path>) -> Self {
        self.cert = Box::new(LazyFile {
            path: path.as_ref().into(),
            file: None,
        });
        self
    }

    /// sets the Tls certificate via bytes slice
    pub fn with_cert(mut self, cert: &[u8]) -> Self {
        self.cert = Box::new(Cursor::new(Vec::from(cert)));
        self
    }

    /// Sets the trust anchor for optional Tls client authentication via file path.
    ///
    /// Anonymous and authenticated clients will be accepted. If no trust anchor is provided by any
    /// of the `client_auth_` methods, then client authentication is disabled by default.
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
    pub fn with_client_auth_optional(mut self, trust_anchor: &[u8]) -> Self {
        let cursor = Box::new(Cursor::new(Vec::from(trust_anchor)));
        self.client_auth = TlsClientAuth::Optional(cursor);
        self
    }

    /// Sets the trust anchor for required Tls client authentication via file path.
    ///
    /// Only authenticated clients will be accepted. If no trust anchor is provided by any of the
    /// `client_auth_` methods, then client authentication is disabled by default.
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
    pub fn with_client_auth_required(mut self, trust_anchor: &[u8]) -> Self {
        let cursor = Box::new(Cursor::new(Vec::from(trust_anchor)));
        self.client_auth = TlsClientAuth::Required(cursor);
        self
    }

    /// sets the DER-encoded OCSP response
    pub fn with_ocsp_resp(mut self, ocsp_resp: &[u8]) -> Self {
        self.ocsp_resp = Vec::from(ocsp_resp);
        self
    }

    /// Build new `TlsListener`
    pub fn bind(self, addr: impl Into<StdSocketAddr>) -> Result<TlsListener, TlsError> {
        let mut incoming = AddrIncoming::bind(&addr.into()).map_err(TlsError::Hyper)?;
        incoming.set_nodelay(true);
        let config = self.build_config()?;
        Ok(TlsListener::new(config, incoming))
    }

    pub(crate) fn build_config(mut self) -> Result<ServerConfig, TlsError> {
        let mut cert_rdr = BufReader::new(self.cert);
        let cert_chain = rustls_pemfile::certs(&mut cert_rdr)
            .map_err(|_| TlsError::CertParseError)?
            .into_iter()
            .map(Certificate)
            .collect();

        let key = {
            // convert it to Vec<u8> to allow reading it again if key is RSA
            let mut key_vec = Vec::new();
            self.key.read_to_end(&mut key_vec).map_err(TlsError::Io)?;

            if key_vec.is_empty() {
                return Err(TlsError::EmptyKey);
            }

            let mut pkcs8 = pkcs8_private_keys(&mut key_vec.as_slice()).map_err(|_| TlsError::Pkcs8ParseError)?;

            if !pkcs8.is_empty() {
                pkcs8.remove(0)
            } else {
                let mut rsa = rsa_private_keys(&mut key_vec.as_slice()).map_err(|_| TlsError::RsaParseError)?;

                if !rsa.is_empty() {
                    rsa.remove(0)
                } else {
                    return Err(TlsError::EmptyKey);
                }
            }
        };

        fn read_trust_anchor(trust_anchor: Box<dyn Read + Send + Sync>) -> Result<RootCertStore, TlsError> {
            let mut reader = BufReader::new(trust_anchor);
            let certs = rustls_pemfile::certs(&mut reader).map_err(|_| TlsError::RsaParseError)?;
            let mut store = RootCertStore::empty();
            if let (0, _) = store.add_parsable_certificates(&certs) {
                Err(TlsError::CertParseError)
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
            .map_err(|_| TlsError::RsaParseError)?
            .with_client_cert_verifier(client_auth)
            .with_single_cert_with_ocsp_and_sct(cert_chain, PrivateKey(key), self.ocsp_resp, Vec::new())
            .map_err(TlsError::InvalidKey)?;
        Ok(config)
    }
}

/// TlsListener
pub struct TlsListener {
    config: Arc<ServerConfig>,
    incoming: AddrIncoming,
}

impl TlsListener {
    /// Create new `TlsListener`.
    pub fn new<C>(config: C, incoming: AddrIncoming) -> Self
    where
        C: Into<Arc<ServerConfig>>,
    {
        TlsListener {
            config: config.into(),
            incoming,
        }
    }

    /// Returns `TlsListenerBuilder`
    pub fn builder() -> TlsListenerBuilder {
        TlsListenerBuilder::new()
    }
}

impl Listener for TlsListener {}
impl Accept for TlsListener {
    type Conn = TlsStream;
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let pin = self.get_mut();
        match ready!(Pin::new(&mut pin.incoming).poll_accept(cx)) {
            Some(Ok(sock)) => Poll::Ready(Some(Ok(TlsStream::new(sock, pin.config.clone())))),
            Some(Err(e)) => Poll::Ready(Some(Err(e))),
            None => Poll::Ready(None),
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
/// TlsStream implements AsyncRead/AsyncWrite handshaking tokio_rustls::Accept first
pub struct TlsStream {
    state: State,
    remote_addr: SocketAddr,
}
impl Transport for TlsStream {
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr.clone())
    }
}

impl TlsStream {
    fn new(stream: AddrStream, config: Arc<ServerConfig>) -> Self {
        let remote_addr = stream.remote_addr();
        let accept = tokio_rustls::TlsAcceptor::from(config).accept(stream);
        TlsStream {
            state: State::Handshaking(accept),
            remote_addr: remote_addr.into(),
        }
    }
}

impl AsyncRead for TlsStream {
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

impl AsyncWrite for TlsStream {
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
        TlsListener::builder()
            .with_key_path("../examples/tls/key.rsa")
            .with_cert_path("../examples/tls/cert.pem")
            .build_config()
            .unwrap();
    }

    #[test]
    fn bytes_cert_key() {
        let key = include_str!("../../../examples/tls/key.rsa");
        let cert = include_str!("../../../examples/tls/cert.pem");

        TlsListener::builder()
            .with_key(key.as_bytes())
            .with_cert(cert.as_bytes())
            .build_config()
            .unwrap();
    }
}
