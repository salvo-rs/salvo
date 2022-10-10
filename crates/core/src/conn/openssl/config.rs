//! openssl module
use std::fmt::{self, Formatter};
use std::fs::File;
use std::io::{self, Error as IoError, Read};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::Ready;
use futures_util::{ready, stream, Stream};
use openssl::pkey::PKey;
use openssl::ssl::{Ssl, SslAcceptor, SslAcceptorBuilder, SslMethod, SslRef};
use openssl::x509::X509;
use pin_project::pin_project;
use tokio::net::{ToSocketAddrs, TcpListener as TokioTcpListener};
use tokio::io::{AsyncRead, AsyncWrite, ErrorKind, ReadBuf};
use tokio_openssl::SslStream;

use super::{Acceptor, Listener, Accepted};

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
    pub fn create_acceptor_builder(&self) -> Result<SslAcceptorBuilder, IoError> {
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


impl<T> IntoConfigStream<OpensslConfig> for T
where
    T: Stream<Item = OpensslConfig> + Send + 'static,
{
    type Stream = Self;

    fn into_stream(self) -> IoResult<Self::Stream> {
        Ok(self)
    }
}

impl IntoConfigStream<OpensslConfig> for OpensslConfig {
    type Stream = futures_util::stream::Once<futures_util::future::Ready<RustlsConfig>>;

    fn into_stream(self) -> IoResult<Self::Stream> {
        let _ = self.create_acceptor_builder()?;
        Ok(futures_util::stream::once(futures_util::future::ready(
            self,
        )))
    }
}
