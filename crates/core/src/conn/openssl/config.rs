//! openssl module
use std::fmt::{self, Debug, Formatter};
use std::fs::File;
use std::future::{Ready, ready};
use std::io::{Error as IoError, Read, Result as IoResult};
use std::path::Path;

use futures_util::stream::{Once, Stream, once};
use openssl::pkey::PKey;
use openssl::ssl::{SslAcceptor, SslMethod};
use openssl::x509::X509;
use tokio::io::ErrorKind;

use crate::conn::IntoConfigStream;

pub use openssl::ssl::SslAcceptorBuilder;

/// Private key and certificate
#[derive(Debug)]
pub struct Keycert {
    key: Vec<u8>,
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
            key: vec![],
            cert: vec![],
        }
    }
    /// Sets the Tls private key via File Path, returns [`IoError`] if the file cannot be open.
    #[inline]
    pub fn key_from_path(mut self, path: impl AsRef<Path>) -> IoResult<Self> {
        let mut file = File::open(path.as_ref())?;
        file.read_to_end(&mut self.key)?;
        Ok(self)
    }

    /// Sets the Tls private key via bytes slice.
    #[inline]
    pub fn with_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.key = key.into();
        self
    }

    /// Specify the file path for the TLS certificate to use.
    #[inline]
    pub fn cert_from_path(mut self, path: impl AsRef<Path>) -> IoResult<Self> {
        let mut file = File::open(path.as_ref())?;
        file.read_to_end(&mut self.cert)?;
        Ok(self)
    }

    /// Sets the Tls certificate via bytes slice
    #[inline]
    pub fn with_cert(mut self, cert: impl Into<Vec<u8>>) -> Self {
        self.cert = cert.into();
        self
    }

    /// Get the private key.
    #[inline]
    pub fn key(&mut self) -> IoResult<&[u8]> {
        if self.key.is_empty() {
            Err(IoError::new(ErrorKind::Other, "empty key"))
        } else {
            Ok(&self.key)
        }
    }

    /// Get the cert.
    #[inline]
    pub fn cert(&mut self) -> IoResult<&[u8]> {
        if self.cert.is_empty() {
            Err(IoError::new(ErrorKind::Other, "empty cert"))
        } else {
            Ok(&self.cert)
        }
    }
}

fn alpn_protocols() -> Vec<u8> {
    #[allow(unused_mut)]
    let mut alpn_protocols: Vec<Vec<u8>> = Vec::with_capacity(3);
    #[cfg(feature = "quinn")]
    alpn_protocols.push(b"\x02h3".to_vec());
    #[cfg(feature = "http2")]
    alpn_protocols.push(b"\x02h2".to_vec());
    #[cfg(feature = "http1")]
    alpn_protocols.push(b"\x08http/1.1".to_vec());
    alpn_protocols.into_iter().flatten().collect()
}

type BuilderModifier = Box<dyn FnMut(&mut SslAcceptorBuilder) + Send + 'static>;
/// Builder to set the configuration for the Tls server.
#[non_exhaustive]
pub struct OpensslConfig {
    /// Key and certificate.
    pub keycert: Keycert,
    /// Builder modifier.
    pub builder_modifier: Option<BuilderModifier>,
    /// Protocols through ALPN (Application-Layer Protocol Negotiation).
    pub alpn_protocols: Vec<u8>,
}

impl Debug for OpensslConfig {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("OpensslConfig").finish()
    }
}

impl OpensslConfig {
    /// Create new `OpensslConfig`
    #[inline]
    pub fn new(keycert: Keycert) -> Self {
        OpensslConfig {
            keycert,
            builder_modifier: None,
            alpn_protocols: alpn_protocols(),
        }
    }

    /// Set builder modifier.
    pub fn builder_modifier<F>(mut self, modifier: F) -> Self
    where
        F: FnMut(&mut SslAcceptorBuilder) + Send + 'static,
    {
        self.builder_modifier = Some(Box::new(modifier));
        self
    }

    /// Set specific protocols through ALPN (Application-Layer Protocol Negotiation).
    #[inline]
    pub fn alpn_protocols(mut self, alpn_protocols: impl Into<Vec<u8>>) -> Self {
        self.alpn_protocols = alpn_protocols.into();
        self
    }

    /// Create [`SslAcceptorBuilder`]
    pub fn create_acceptor_builder(&mut self) -> IoResult<SslAcceptorBuilder> {
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
        let alpn_protocols = self.alpn_protocols.clone();
        builder.set_alpn_protos(&self.alpn_protocols)?;
        // set uo ALPN selection routine - as select_next_proto
        builder.set_alpn_select_callback(move |_, list| {
            let proto = openssl::ssl::select_next_proto(&alpn_protocols, list)
                .ok_or(openssl::ssl::AlpnError::NOACK)?;
            let pos = list
                .windows(proto.len())
                .position(|window| window == proto)
                .expect("selected alpn proto should be present in client protos");
            Ok(&list[pos..pos + proto.len()])
        });
        if let Some(modifier) = &mut self.builder_modifier {
            modifier(&mut builder);
        }
        Ok(builder)
    }
}

impl TryInto<SslAcceptorBuilder> for OpensslConfig {
    type Error = IoError;

    fn try_into(mut self) -> IoResult<SslAcceptorBuilder> {
        self.create_acceptor_builder()
    }
}

impl IntoConfigStream<OpensslConfig> for OpensslConfig {
    type Stream = Once<Ready<OpensslConfig>>;

    fn into_stream(self) -> Self::Stream {
        once(ready(self))
    }
}
impl<T> IntoConfigStream<OpensslConfig> for T
where
    T: Stream<Item = OpensslConfig> + Send + 'static,
{
    type Stream = T;

    fn into_stream(self) -> Self {
        self
    }
}

impl<T> IntoConfigStream<SslAcceptorBuilder> for T
where
    T: Stream<Item = SslAcceptorBuilder> + Send + 'static,
{
    type Stream = T;

    fn into_stream(self) -> Self {
        self
    }
}
