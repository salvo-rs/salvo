//! tls module
use std::future::Future;
use std::io::{self, Cursor, Read};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::future::Ready;
use futures_util::{ready, stream, Stream};
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use pin_project_lite::pin_project;
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

    /// generate identity
    #[inline]
    pub fn identity(mut self) -> Result<Identity, io::Error> {
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
impl<C> NativeTlsListener<C> {
    /// Get local address
    pub fn local_addr(&self) -> SocketAddr {
        self.incoming.local_addr().into()
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
    type Conn = NativeTlsStream;
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let this = self.project();
        if let Poll::Ready(result) = this.config_stream.poll_next(cx) {
            if let Some(identity) = result {
                let identity = identity.into();
                *this.acceptor = Some(
                    TlsAcceptor::new(identity)
                        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?
                        .into(),
                );
            }
        }
        if let Some(acceptor) = this.acceptor {
            match ready!(Pin::new(this.incoming).poll_accept(cx)) {
                Some(Ok(sock)) => {
                    let acceptor = acceptor.clone();
                    let stream =
                        NativeTlsStream::new(sock.remote_addr().into(), async move { acceptor.accept(sock).await });
                    Poll::Ready(Some(Ok(stream)))
                }
                Some(Err(e)) => Poll::Ready(Some(Err(e))),
                _ => Poll::Ready(None),
            }
        } else {
            Poll::Ready(Some(Err(io::Error::new(io::ErrorKind::Other, "acceptor is none"))))
        }
    }
}

pin_project! {
    /// NativeTlsStream
    #[cfg_attr(docsrs, doc(cfg(feature = "native_tls")))]
    pub struct NativeTlsStream {
        #[pin]
        inner_future: Pin<Box<dyn Future<Output=Result<TlsStream<AddrStream>, tokio_native_tls::native_tls::Error>> + Send>>,
        remote_addr: SocketAddr,
    }
}
impl Transport for NativeTlsStream {
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr.clone())
    }
}

impl NativeTlsStream {
    fn new(
        remote_addr: SocketAddr,
        inner_future: impl Future<Output = Result<TlsStream<AddrStream>, tokio_native_tls::native_tls::Error>>
            + Send
            + 'static,
    ) -> Self {
        NativeTlsStream {
            inner_future: Box::pin(inner_future),
            remote_addr,
        }
    }
}

impl AsyncRead for NativeTlsStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let this = self.project();
        if let Ok(mut stream) = ready!(this.inner_future.poll(cx)) {
            Pin::new(&mut stream).poll_read(cx, buf)
        } else {
            Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "native tls error")))
        }
    }
}

impl AsyncWrite for NativeTlsStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.project();
        if let Ok(mut stream) = ready!(this.inner_future.poll(cx)) {
            Pin::new(&mut stream).poll_write(cx, buf)
        } else {
            Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "native tls error")))
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.project();
        if let Ok(mut stream) = ready!(this.inner_future.poll(cx)) {
            Pin::new(&mut stream).poll_flush(cx)
        } else {
            Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "native tls error")))
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.project();
        if let Ok(mut stream) = ready!(this.inner_future.poll(cx)) {
            Pin::new(&mut stream).poll_shutdown(cx)
        } else {
            Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "native tls error")))
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use http::Request;
//     use hyper::client::conn::handshake;
//     use hyper::Body;
//     use tokio::io::{AsyncReadExt, AsyncWriteExt};
//     use tokio::net::TcpStream;
//     use tokio_native_tls::native_tls::TlsConnector;
//     use tower::{Service, ServiceExt};

//     use super::*;
//     use crate::prelude::*;

//     #[tokio::test]
//     async fn test_native_tls_listener() {
//         #[fn_handler]
//         async fn hello_world() -> &'static str {
//             "Hello World"
//         }
//         let addr = "127.0.0.1:7879";
//         let listener = NativeTlsListener::with_config(
//             NativeTlsConfig::new()
//                 .with_pkcs12(include_bytes!("../../../examples/certs/identity.p12").to_vec())
//                 .with_password("mypass"),
//         )
//         .bind(addr);
//         let router = Router::new().get(hello_world);
//         let server = tokio::task::spawn(async {
//             Server::new(listener).serve(router).await;
//         });
//         tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

//         let socket = TcpStream::connect(&addr).await.unwrap();
//         let cx = tokio_native_tls::TlsConnector::from(
//             tokio_native_tls::native_tls::TlsConnector::builder()
//                 .danger_accept_invalid_certs(true)
//                 .build()
//                 .unwrap(),
//         );
//         let mut socket = cx.connect(addr, socket).await.unwrap();
//         socket
//             .write_all(
//                 "\
//                  GET / HTTP/1.0\r\n\
//                  Host: 127.0.0.1\r\n\
//                  \r\n\
//                  "
//                 .as_bytes(),
//             )
//             .await
//             .unwrap();
//         let mut data = Vec::new();
//         socket.read_to_end(&mut data).await.unwrap();
//         server.abort();

//         assert_eq!(String::from_utf8_lossy(&data[..]), "Hello World");
//     }
// }
