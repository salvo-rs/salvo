//! rustls module
use std::collections::HashMap;
use std::fmt::{self, Formatter};
use std::fs::File;
use std::future::Future;
use std::io::{self, Error as IoError, Read};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::Ready;
use futures_util::{ready, stream, Stream};
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ErrorKind, ReadBuf};
pub use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::rustls::server::{
    AllowAnyAnonymousOrAuthenticatedClient, AllowAnyAuthenticatedClient, ClientHello, NoClientAuth, ResolvesServerCert,
};
use tokio_rustls::rustls::sign::{self, CertifiedKey};
use tokio_rustls::rustls::{Certificate, PrivateKey, RootCertStore};

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
    ocsp_resp: Vec<u8>,
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
            ocsp_resp: vec![],
        }
    }
    /// Sets the Tls private key via File Path, returns `IoError` if the file cannot be open.
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

    /// Get ocsp_resp.
    #[inline]
    pub fn ocsp_resp(&self) -> &[u8] {
        &self.ocsp_resp
    }

    fn build_certified_key(&mut self) -> io::Result<CertifiedKey> {
        let cert = rustls_pemfile::certs(&mut self.cert()?)
            .map(|mut certs| certs.drain(..).map(Certificate).collect())
            .map_err(|_| IoError::new(ErrorKind::Other, "failed to parse tls certificates"))?;

        let key = {
            let mut pkcs8 = rustls_pemfile::pkcs8_private_keys(&mut self.key()?)
                .map_err(|_| IoError::new(ErrorKind::Other, "failed to parse tls private keys"))?;
            if !pkcs8.is_empty() {
                PrivateKey(pkcs8.remove(0))
            } else {
                let mut rsa = rustls_pemfile::rsa_private_keys(&mut self.key()?)
                    .map_err(|_| IoError::new(ErrorKind::Other, "failed to parse tls private keys"))?;

                if !rsa.is_empty() {
                    PrivateKey(rsa.remove(0))
                } else {
                    return Err(IoError::new(ErrorKind::Other, "failed to parse tls private keys"));
                }
            }
        };

        let key = sign::any_supported_type(&key).map_err(|_| IoError::new(ErrorKind::Other, "invalid private key"))?;

        Ok(CertifiedKey {
            cert,
            key,
            ocsp: if !self.ocsp_resp.is_empty() {
                Some(self.ocsp_resp.clone())
            } else {
                None
            },
            sct_list: None,
        })
    }
}

/// Tls client authentication configuration.
pub(crate) enum TlsClientAuth {
    /// No client auth.
    Off,
    /// Allow any anonymous or authenticated client.
    Optional(Vec<u8>),
    /// Allow any authenticated client.
    Required(Vec<u8>),
}

/// Builder to set the configuration for the Tls server.
pub struct RustlsConfig {
    fallback: Option<Keycert>,
    keycerts: HashMap<String, Keycert>,
    client_auth: TlsClientAuth,
}

impl fmt::Debug for RustlsConfig {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("RustlsConfig").finish()
    }
}

impl RustlsConfig {
    /// Create new `RustlsConfig`
    #[inline]
    pub fn new(fallback: impl Into<Option<Keycert>>) -> Self {
        RustlsConfig {
            fallback: fallback.into(),
            keycerts: HashMap::new(),
            client_auth: TlsClientAuth::Off,
        }
    }

    /// Sets the trust anchor for optional Tls client authentication via file path.
    ///
    /// Anonymous and authenticated clients will be accepted. If no trust anchor is provided by any
    /// of the `client_auth_` methods, then client authentication is disabled by default.
    #[inline]
    pub fn client_auth_optional_path(mut self, path: impl AsRef<Path>) -> io::Result<Self> {
        let mut data = vec![];
        let mut file = File::open(path)?;
        file.read_to_end(&mut data)?;
        self.client_auth = TlsClientAuth::Optional(data);
        Ok(self)
    }

    /// Sets the trust anchor for optional Tls client authentication via bytes slice.
    ///
    /// Anonymous and authenticated clients will be accepted. If no trust anchor is provided by any
    /// of the `client_auth_` methods, then client authentication is disabled by default.
    pub fn client_auth_optional(mut self, trust_anchor: impl Into<Vec<u8>>) -> Self {
        self.client_auth = TlsClientAuth::Optional(trust_anchor.into());
        self
    }

    /// Sets the trust anchor for required Tls client authentication via file path.
    ///
    /// Only authenticated clients will be accepted. If no trust anchor is provided by any of the
    /// `client_auth_` methods, then client authentication is disabled by default.
    #[inline]
    pub fn client_auth_required_path(mut self, path: impl AsRef<Path>) -> io::Result<Self> {
        let mut data = vec![];
        let mut file = File::open(path)?;
        file.read_to_end(&mut data)?;
        self.client_auth = TlsClientAuth::Required(data);
        Ok(self)
    }

    /// Sets the trust anchor for required Tls client authentication via bytes slice.
    ///
    /// Only authenticated clients will be accepted. If no trust anchor is provided by any of the
    /// `client_auth_` methods, then client authentication is disabled by default.
    #[inline]
    pub fn client_auth_required(mut self, trust_anchor: impl Into<Vec<u8>>) -> Self {
        self.client_auth = TlsClientAuth::Required(trust_anchor.into());
        self
    }

    /// Add a new keycert to be used for the given SNI `name`.
    #[inline]
    pub fn keycert(mut self, name: impl Into<String>, keycert: Keycert) -> Self {
        self.keycerts.insert(name.into(), keycert);
        self
    }
    /// ServerConfig
    fn build_server_config(mut self) -> io::Result<ServerConfig> {
        let fallback = self
            .fallback
            .as_mut()
            .map(|fallback| fallback.build_certified_key())
            .transpose()?
            .map(Arc::new);
        let mut certified_keys = HashMap::new();

        for (name, keycert) in &mut self.keycerts {
            certified_keys.insert(name.clone(), Arc::new(keycert.build_certified_key()?));
        }

        let client_auth = match &self.client_auth {
            TlsClientAuth::Off => NoClientAuth::new(),
            TlsClientAuth::Optional(trust_anchor) => {
                AllowAnyAnonymousOrAuthenticatedClient::new(read_trust_anchor(trust_anchor)?)
            }
            TlsClientAuth::Required(trust_anchor) => AllowAnyAuthenticatedClient::new(read_trust_anchor(trust_anchor)?),
        };

        let mut config = ServerConfig::builder()
            .with_safe_defaults()
            .with_client_cert_verifier(client_auth)
            .with_cert_resolver(Arc::new(CertResolver { certified_keys, fallback }));
        config.alpn_protocols = vec!["h2".into(), "http/1.1".into()];
        Ok(config)
    }
}

struct CertResolver {
    fallback: Option<Arc<CertifiedKey>>,
    certified_keys: HashMap<String, Arc<CertifiedKey>>,
}

impl ResolvesServerCert for CertResolver {
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        client_hello
            .server_name()
            .and_then(|name| self.certified_keys.get(name).map(Arc::clone))
            .or_else(|| self.fallback.clone())
    }
}

#[inline]
fn read_trust_anchor(mut trust_anchor: &[u8]) -> io::Result<RootCertStore> {
    let certs = rustls_pemfile::certs(&mut trust_anchor)?;
    let mut store = RootCertStore::empty();
    for cert in certs {
        store
            .add(&Certificate(cert))
            .map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
    }
    Ok(store)
}

/// RustlsListener
#[pin_project]
pub struct RustlsListener<C> {
    #[pin]
    config_stream: C,
    incoming: AddrIncoming,
    server_config: Option<Arc<ServerConfig>>,
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
            incoming: incoming.into_incoming(),
            server_config: None,
        })
    }
}

impl<C> RustlsListener<C> {
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
impl RustlsListener<stream::Once<Ready<Arc<ServerConfig>>>> {
    /// Create new RustlsListenerBuilder with RustlsConfig.
    #[inline]
    pub fn with_config(config: RustlsConfig) -> RustlsListenerBuilder<stream::Once<Ready<Arc<ServerConfig>>>> {
        Self::try_with_config(config).unwrap()
    }
    /// Try to create new RustlsListenerBuilder with RustlsConfig.
    #[inline]
    pub fn try_with_config(
        config: RustlsConfig,
    ) -> io::Result<RustlsListenerBuilder<stream::Once<Ready<Arc<ServerConfig>>>>> {
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
    #[inline]
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

    #[inline]
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
                "failed to load rustls server config",
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
pub struct RustlsStream {
    state: RustlsState,
    remote_addr: SocketAddr,
}
impl Transport for RustlsStream {
    #[inline]
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr.clone())
    }
}

impl RustlsStream {
    #[inline]
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
    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        match pin.state {
            RustlsState::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_read(cx, buf);
                    pin.state = RustlsState::Streaming(stream);
                    result
                }
                Err(e) => Poll::Ready(Err(e)),
            },
            RustlsState::Streaming(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for RustlsStream {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let pin = self.get_mut();
        match pin.state {
            RustlsState::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_write(cx, buf);
                    pin.state = RustlsState::Streaming(stream);
                    result
                }
                Err(e) => Poll::Ready(Err(e)),
            },
            RustlsState::Streaming(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            RustlsState::Handshaking(_) => Poll::Ready(Ok(())),
            RustlsState::Streaming(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    #[inline]
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

    #[tokio::test]
    async fn test_rustls_listener() {
        let mut listener = RustlsListener::with_config(
            RustlsConfig::new( Keycert::new()
            .with_key_path("certs/key.pem")
            .with_cert_path("certs/cert.pem")),
        )
        .bind("127.0.0.1:0");
        let addr = listener.local_addr();

        tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            let trust_anchor = include_bytes!("../../certs/chain.pem");
            let client_config = ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(read_trust_anchor(trust_anchor.as_slice()).unwrap())
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
