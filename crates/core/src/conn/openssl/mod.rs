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
use tokio::io::{AsyncRead, AsyncWrite, ErrorKind, ReadBuf};
use tokio::net::{TcpListener as TokioTcpListener, ToSocketAddrs};
use tokio_openssl::SslStream;

use crate::conn::{Accepted, Acceptor, Listener, IntoConfigStream, HandshakeStream};

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
    openssl_config: Option<OpensslConfig>,
    acceptor: Option<Arc<SslAcceptor>>,
    inner: TokioTcpListener,
    local_addr: SocketAddr,
}

/// OpensslListener
pub struct OpensslListenerBuilder<C> {
    config_stream: C,
}
impl<C> OpensslListenerBuilder<C>
where
    C: IntoConfigStream<OpensslConfig>,
{
    /// Bind to socket address.
    #[inline]
    pub fn bind(config_stream: C, addr: impl ToSocketAddrs) -> OpensslListener<C> {
        Self::try_bind(config_stream, addr).unwrap()
    }
    /// Try to bind to socket address.
    #[inline]
    pub fn try_bind(config_stream: C, addr: impl ToSocketAddrs) -> Result<OpensslListener<C>, hyper::Error> {
        let inner = TokioTcpListener::bind(addr).await?;
        let local_addr: SocketAddr = inner.local_addr()?.into();
        Ok(OpensslListener {
            config_stream,
            openssl_config: None,
            acceptor: None,
            inner,
            local_addr,
        })
    }
}


#[async_trait]
impl<C, T> Acceptor for OpensslListener<C, T>
where
    C: IntoConfigStream<OpensslConfig>,
    T: Acceptor,
{
    type Conn = HandshakeStream<SslStream<T::Conn>>;
    type Error = IoError;

    #[inline]
    fn local_addrs(&self) -> Vec<&SocketAddr> {
        self.inner.local_addrs()
    }

    #[inline]
    async fn accept(&self) -> Result<Accepted<Self::Conn>, Self::Error> {
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

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use futures_util::{Stream, StreamExt};
    use openssl::ssl::SslConnector;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

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

        let Accepted { mut stream, .. } = listener.next().await.unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 518);
    }
}
