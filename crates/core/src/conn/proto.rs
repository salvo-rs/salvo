use std::cmp;
use std::error::Error as StdError;
use std::io::{Error as IoError, ErrorKind, IoSlice, Result as IoResult};
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{self, Context, Poll, ready};

use bytes::{Buf, Bytes};

use http::{Request, Response, Version};
use hyper::service::Service;
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

use crate::fuse::ArcFusewire;
use crate::http::body::{Body, HyperBody};
#[cfg(any(feature = "http1", feature = "http2"))]
use crate::rt::tokio::TokioIo;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(feature = "http2")]
use crate::rt::tokio::TokioExecutor;
#[cfg(feature = "http1")]
use hyper::server::conn::http1;
#[cfg(feature = "http2")]
use hyper::server::conn::http2;

#[cfg(feature = "quinn")]
use crate::conn::quinn;

const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

#[doc(hidden)]
pub struct HttpBuilder {
    #[cfg(feature = "http1")]
    pub(crate) http1: http1::Builder,
    #[cfg(feature = "http2")]
    pub(crate) http2: http2::Builder<TokioExecutor>,
    #[cfg(feature = "quinn")]
    pub(crate) quinn: quinn::Builder,
}
impl Default for HttpBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpBuilder {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "http1")]
            http1: http1::Builder::new(),
            #[cfg(feature = "http2")]
            http2: http2::Builder::new(crate::rt::tokio::TokioExecutor::new()),
            #[cfg(feature = "quinn")]
            quinn: crate::conn::quinn::Builder::new(),
        }
    }

    /// Serve a connection with the given service.
    #[allow(unused_variables)]
    pub async fn serve_connection<I, S, B>(
        &self,
        socket: I,
        service: S,
        fusewire: Option<ArcFusewire>,
        graceful_stop_token: Option<CancellationToken>,
    ) -> Result<()>
    where
        S: Service<Request<HyperBody>, Response = Response<B>> + Send,
        S::Future: Send + 'static,
        S::Error: Into<Box<dyn StdError + Send + Sync>>,
        B: Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<Box<dyn StdError + Send + Sync>>,
        I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        #[cfg(all(feature = "http1", feature = "http2"))]
        let (version, socket) = if let Some(fusewire) = &fusewire {
            tokio::select! {
                result = read_version(socket) => {
                    result?
                },
                _ = fusewire.fused() => {
                    tracing::info!("closing connection due to fused");
                    return Ok(());
                },
            }
        } else {
            read_version(socket).await?
        };
        #[cfg(all(not(feature = "http1"), not(feature = "http2")))]
        let version = Version::HTTP_11; // Just make the compiler happy.
        #[cfg(all(feature = "http1", not(feature = "http2")))]
        let version = Version::HTTP_11;
        #[cfg(all(not(feature = "http1"), feature = "http2"))]
        let version = Version::HTTP_2;

        match version {
            Version::HTTP_10 | Version::HTTP_11 => {
                #[cfg(not(feature = "http1"))]
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "http1 feature not enabled",
                )
                .into());
                #[cfg(feature = "http1")]
                {
                    let mut conn = self
                        .http1
                        .serve_connection(TokioIo::new(socket), service)
                        .with_upgrades();

                    match (fusewire, graceful_stop_token) {
                        (Some(fusewire), Some(graceful_stop_token)) => {
                            tokio::select! {
                                _ = &mut conn => {
                                    // Connection completed successfully.
                                    return Ok(());
                                },
                                _ = fusewire.fused() => {
                                    tracing::info!("closing connection due to fused");
                                },
                                _ = graceful_stop_token.cancelled() => {
                                    tracing::info!("closing connection due to inactivity");

                                    // Init graceful shutdown for connection (`GOAWAY` for `HTTP/2` or disabling `keep-alive` for `HTTP/1`)
                                    Pin::new(&mut conn).graceful_shutdown();
                                    let _ = conn.await;
                                }
                            }
                        }
                        (None, Some(graceful_stop_token)) => {
                            tokio::select! {
                                _ = &mut conn => {
                                    // Connection completed successfully.
                                    return Ok(());
                                },
                                _ = graceful_stop_token.cancelled() => {
                                    tracing::info!("closing connection due to inactivity");

                                    // Init graceful shutdown for connection (`GOAWAY` for `HTTP/2` or disabling `keep-alive` for `HTTP/1`)
                                    Pin::new(&mut conn).graceful_shutdown();
                                    let _ = conn.await;
                                }
                            }
                        }
                        (Some(fusewire), None) => {
                            tokio::select! {
                                _ = &mut conn => {
                                    // Connection completed successfully.
                                    return Ok(());
                                },
                                _ = fusewire.fused() => {
                                    tracing::info!("closing connection due to fused");
                                }
                            }
                        }
                        (None, None) => {
                            let _ = conn.await;
                        }
                    }
                }
            }
            Version::HTTP_2 => {
                #[cfg(not(feature = "http2"))]
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "http2 feature not enabled",
                )
                .into());
                #[cfg(feature = "http2")]
                {
                    let mut conn = self.http2.serve_connection(TokioIo::new(socket), service);

                    match (fusewire, graceful_stop_token) {
                        (Some(fusewire), Some(graceful_stop_token)) => {
                            tokio::select! {
                                _ = &mut conn => {
                                    // Connection completed successfully.
                                    return Ok(());
                                },
                                _ = fusewire.fused() => {
                                    tracing::info!("closing connection due to fused");
                                },
                                _ = graceful_stop_token.cancelled() => {
                                    tracing::info!("closing connection due to inactivity");

                                    // Init graceful shutdown for connection (`GOAWAY` for `HTTP/2` or disabling `keep-alive` for `HTTP/1`)
                                    Pin::new(&mut conn).graceful_shutdown();
                                    let _ = conn.await;
                                }
                            }
                        }
                        (None, Some(graceful_stop_token)) => {
                            tokio::select! {
                                _ = &mut conn => {
                                    // Connection completed successfully.
                                    return Ok(());
                                },
                                _ = graceful_stop_token.cancelled() => {
                                    tracing::info!("closing connection due to inactivity");

                                    // Init graceful shutdown for connection (`GOAWAY` for `HTTP/2` or disabling `keep-alive` for `HTTP/1`)
                                    Pin::new(&mut conn).graceful_shutdown();
                                    let _ = conn.await;
                                }
                            }
                        }
                        (Some(fusewire), None) => {
                            tokio::select! {
                                _ = &mut conn => {
                                    // Connection completed successfully.
                                    return Ok(());
                                },
                                _ = fusewire.fused() => {
                                    tracing::info!("closing connection due to fused");
                                }
                            }
                        }
                        (None, None) => {
                            let _ = conn.await;
                        }
                    }
                }
            }
            _ => {
                tracing::info!("unsupported protocol version: {:?}", version);
            }
        }

        Ok(())
    }
}

#[allow(dead_code)]
#[allow(clippy::future_not_send)]
pub(crate) async fn read_version<A>(mut reader: A) -> IoResult<(Version, Rewind<A>)>
where
    A: AsyncRead + Unpin,
{
    let mut buf = [0; 24];
    let (version, buf) = ReadVersion {
        reader: &mut reader,
        buf: ReadBuf::new(&mut buf),
        version: Version::HTTP_11,
        _pin: PhantomPinned,
    }
    .await?;
    Ok((version, Rewind::new_buffered(Bytes::from(buf), reader)))
}

#[derive(Debug)]
#[pin_project]
#[must_use = "futures do nothing unless you `.await` or poll them"]
struct ReadVersion<'a, A: ?Sized> {
    reader: &'a mut A,
    buf: ReadBuf<'a>,
    version: Version,
    // Make this future `!Unpin` for compatibility with async trait methods.
    #[pin]
    _pin: PhantomPinned,
}

impl<A> Future for ReadVersion<'_, A>
where
    A: AsyncRead + Unpin + ?Sized,
{
    type Output = IoResult<(Version, Vec<u8>)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<(Version, Vec<u8>)>> {
        let this = self.project();

        while this.buf.remaining() != 0 {
            if this.buf.filled() != &H2_PREFACE[0..this.buf.filled().len()] {
                return Poll::Ready(Ok((*this.version, this.buf.filled().to_vec())));
            }
            // if our buffer is empty, then we need to read some data to continue.
            let rem = this.buf.remaining();
            ready!(Pin::new(&mut *this.reader).poll_read(cx, this.buf))?;
            if this.buf.remaining() == rem {
                return Err(IoError::new(ErrorKind::UnexpectedEof, "early eof")).into();
            }
        }
        if this.buf.filled() == H2_PREFACE {
            *this.version = Version::HTTP_2;
        }
        Poll::Ready(Ok((*this.version, this.buf.filled().to_vec())))
    }
}

// from https://github.com/hyperium/hyper-util/pull/11/files#diff-1bd3ef8e9a23396b76bdb4ec6ab5aba4c48dd0511d287e485148a90170c6b4fd
/// Combine a buffer with an IO, rewinding reads to use the buffer.
#[derive(Debug)]
pub(crate) struct Rewind<T> {
    pre: Option<Bytes>,
    inner: T,
}
#[allow(dead_code)]
impl<T> Rewind<T> {
    fn new_buffered(buf: Bytes, io: T) -> Self {
        Rewind {
            pre: Some(buf),
            inner: io,
        }
    }
}

impl<T> AsyncRead for Rewind<T>
where
    T: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<IoResult<()>> {
        if let Some(mut prefix) = self.pre.take() {
            // If there are no remaining bytes, let the bytes get dropped.
            if !prefix.is_empty() {
                let copy_len = cmp::min(prefix.len(), buf.remaining());
                // TODO: There should be a way to do following two lines cleaner...
                buf.put_slice(&prefix[..copy_len]);
                prefix.advance(copy_len);
                // Put back what's left
                if !prefix.is_empty() {
                    self.pre = Some(prefix);
                }
                return Poll::Ready(Ok(()));
            }
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<T> AsyncWrite for Rewind<T>
where
    T: AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &[u8],
    ) -> Poll<IoResult<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<IoResult<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<IoResult<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<IoResult<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}
