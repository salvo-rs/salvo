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

pub use super::{OpensslConfig, Keycert};

use crate::conn::{Acceptor, Listener, Accepted, HandshakeStream};

/// OpensslListener
#[pin_project]
pub struct OpensslListener<C, T> {
    #[pin]
    config_stream: C,
    openssl_config: Option<OpensslConfig>,
    tls_acceptor: Option<Arc<SslAcceptor>>,
    inner: T,
}

impl<C> OpensslListener<C, TcpListener>
where
    C: Stream,
    C::Item: Into<OpensslConfig>,
{
    /// Bind to socket address.
    #[inline]
    pub fn bind(config: C, addr: impl ToSocketAddrs) -> OpensslListener<C, TcpListener> {
        Self::try_bind(addr).unwrap()
    }
    /// Try to bind to socket address.
    #[inline]
    pub fn try_bind(config: C, addr: impl ToSocketAddrs) -> Result<OpensslListener<C, TcpListener>, hyper::Error> {
       let inner = TokioTcpListener::bind(addr).await?;
        let local_addr: SocketAddr = inner.local_addr()?.into();
        Ok(OpensslListener {
            config_stream: config.into_stream(),
            openssl_config: None,
            acceptor: None,
            inner,
            local_addr,
        })
    }
}

impl<C, T> OpensslListener<C, T>
where
    C: Stream,
    C::Item: Into<OpensslConfig>,
{
    #[inline]
    pub fn new(inner: T, config: C) -> Self {
        Self {
            inner,
            config_stream,
        }
    }
}

#[async_trait]
impl<C, T> Acceptor for OpensslListener<C, T>
where
    C: Stream<Item = OpensslTlsConfig> + Send + Unpin + 'static,
{
    type Conn = HandshakeStream<SslStream<T::Conn>>;
    type Error = IoError;

    /// Get the local address bound to this listener.
    pub fn local_addrs(&self) -> Vec<&SocketAddr> {
        self.inner.local_addrs()
    }

    #[inline]
    async fn accept(&mut self) -> Result<Accepted<Self::Conn>, Self::Error> {
        loop {
            tokio::select! {
                res = self.config_stream.next() => {
                    if let Some(tls_config) = res {
                        match tls_config.create_acceptor_builder() {
                            Ok(builder) => {
                                if self.tls_acceptor.is_some() {
                                    tracing::info!("tls config changed.");
                                } else {
                                    tracing::info!("tls config loaded.");
                                }
                                self.tls_acceptor = Some(Arc::new(builder.build()));
                            },
                            Err(err) => tracing::error!(error = %err, "invalid tls config."),
                        }
                    } else {
                        unreachable!()
                    }
                }
                res = self.inner.accept() => {
                    let (stream, local_addr, remote_addr, _) = res?;
                    let tls_acceptor = match &self.tls_acceptor {
                        Some(tls_acceptor) => tls_acceptor.clone(),
                        None => return Err(IoError::new(ErrorKind::Other, "no valid tls config.")),
                    };
                    let fut = async move {
                        let ssl = Ssl::new(tls_acceptor.context()).map_err(|err|
                            IoError::new(ErrorKind::Other, err.to_string()))?;
                        let mut tls_stream = SslStream::new(ssl, stream).map_err(|err|
                            IoError::new(ErrorKind::Other, err.to_string()))?;
                        use std::pin::Pin;
                        Pin::new(&mut tls_stream).accept().await.map_err(|err|
                            IoError::new(ErrorKind::Other, err.to_string()))?;
                        Ok(tls_stream) };
                    let stream = HandshakeStream::new(fut);
                    return Ok((stream, local_addr, remote_addr, Scheme::HTTPS));
                }
            }
        }
    }
}

/// OpensslStream implements AsyncRead/AsyncWrite handshaking tokio_openssl::Accept first
pub struct OpensslStream {
    inner_stream: SslStream<AddrStream>,
    remote_addr: SocketAddr,
    is_ready: bool,
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
