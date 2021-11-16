//! tls module
use std::fs::File;
use std::future::Future;
use std::io::{self, BufReader, Cursor, Read};
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
use tokio_native_tls::native_tls::{Identity, TlsAcceptor};
use tokio_native_tls::{TlsAcceptor as AsyncTlsAcceptor, TlsStream};

use super::{IntoAddrIncoming, LazyFile, Listener};
use crate::addr::SocketAddr;
use crate::transport::Transport;

/// Builder to set the configuration for the Tls server.
pub struct NativeTlsConfig {
    pkcs12: Box<dyn Read + Send + Sync>,
    password: String,
}

impl std::fmt::Debug for NativeTlsConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_struct("NativeTlsConfig").finish()
    }
}

impl NativeTlsConfig {
    /// Create new `NativeTlsConfig`
    #[inline]
    pub fn new() -> Self {
        NativeTlsConfig {
            pkcs12: Box::new(io::empty()),
            password: String::new(),
        }
    }

    /// sets the pkcs12 via File Path, returns `Error::IoError` if the file cannot be open
    #[inline]
    pub fn with_pkcs12_path(mut self, path: impl AsRef<Path>) -> Self {
        self.pkcs12 = Box::new(LazyFile {
            path: path.as_ref().into(),
            file: None,
        });
        self
    }

    /// sets the pkcs12 via bytes slice
    #[inline]
    pub fn with_pkcs12(mut self, pkcs12: impl Into<Vec<u8>>) -> Self {
        self.pkcs12 = Box::new(Cursor::new(pkcs12.into()));
        self
    }
    /// sets the password
    #[inline]
    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = password.into();
        self
    }
    #[inline]
    pub fn identity(&self) -> Result<Identity, io::Error> {
        let mut pkcs12 = Vec::new();
        self.pkcs12
            .read_to_end(&mut pkcs12)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
        Identity::from_pkcs12(&pkcs12, &self.password)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))
    }
}

pin_project! {
    /// NativeTlsListener
    pub struct NativeTlsListener<C> {
        #[pin]
        config_stream: C,
        incoming: AddrIncoming,
        acceptor: Option<AsyncTlsAcceptor>,
    }
}
/// NativeTlsListener
pub struct NativeTlsListenerBuilder<C> {
    config_stream: C,
}
impl<C> NativeTlsListenerBuilder<C>
where
    C: Stream,
    C::Item: Into<Identity>,
{
    /// Bind to socket address.
    #[inline]
    pub fn bind(self, incoming: impl IntoAddrIncoming) -> NativeTlsListener<C> {
        self.try_bind(incoming).unwrap()
    }
    /// Try to bind to socket address.
    #[inline]
    pub fn try_bind(self, incoming: impl IntoAddrIncoming) -> Result<NativeTlsListener<C>, hyper::Error> {
        Ok(NativeTlsListener {
            config_stream: self.config_stream,
            incoming: incoming.into_incoming(),
            acceptor: None,
        })
    }
}

impl NativeTlsListener<stream::Once<Ready<Identity>>> {
    /// Create new NativeTlsListenerBuilder with NativeTlsConfig.
    #[inline]
    pub fn with_config(config: NativeTlsConfig) -> NativeTlsListenerBuilder<stream::Once<Ready<Identity>>> {
        Self::try_with_config(config).unwrap()
    }
    /// Try to create new NativeTlsListenerBuilder with NativeTlsConfig.
    #[inline]
    pub fn try_with_config(
        config: NativeTlsConfig,
    ) -> Result<NativeTlsListenerBuilder<stream::Once<Ready<Identity>>>, io::Error> {
        let identity = config.identity()?;
        Ok(Self::with_identity(identity))
    }
    /// Create new NativeTlsListenerBuilder with Identity.
    #[inline]
    pub fn with_identity(identity: impl Into<Identity>) -> NativeTlsListenerBuilder<stream::Once<Ready<Identity>>> {
        let stream = futures_util::stream::once(futures_util::future::ready(identity.into()));
        Self::with_config_stream(stream)
    }
}

impl From<NativeTlsConfig> for Identity {
    fn from(config: NativeTlsConfig) -> Self {
        config.identity().unwrap()
    }
}

impl<C> NativeTlsListener<C>
where
    C: Stream,
    C::Item: Into<Identity>,
{
    /// Create new NativeTlsListener with config stream.
    #[inline]
    pub fn with_config_stream(config_stream: C) -> NativeTlsListenerBuilder<C> {
        NativeTlsListenerBuilder { config_stream }
    }
}

impl<C> Listener for NativeTlsListener<C>
where
    C: Stream,
    C::Item: Into<Identity>,
{
}
impl<C> Accept for NativeTlsListener<C>
where
    C: Stream,
    C::Item: Into<Identity>,
{
    type Conn = TlsStream<std::net::TcpStream>;
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let this = self.project();
        if let Poll::Ready(result) = this.config_stream.poll_next(cx) {
            if let Some(identity) = result {
                let identity = identity.into();
                self.acceptor = Some(
                    TlsAcceptor::new(identity)
                        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?
                        .into(),
                );
            }
        }
        match &self.acceptor {
            Some(acceptor) => match ready!(Pin::new(this.incoming).poll_accept(cx)) {
                Some(stream) => match stream {
                    Ok(stream) => match Pin::new(&acceptor.accept(stream)).poll() {
                        Ok(stream) => Poll::Ready(Some(Ok(stream.into()))),
                        Err(_) => Poll::Ready(Some(Err(io::Error::new(io::ErrorKind::Other, "acceptor is none")))),
                    },
                    Err(e) => Poll::Ready(Some(Err(e))),
                },
                None => Poll::Ready(None),
            },
            None => Poll::Ready(Some(Err(io::Error::new(io::ErrorKind::Other, "acceptor is none")))),
        }
    }
}

impl Transport for TlsStream<AddrStream> {
    fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote_addr()
    }
}
