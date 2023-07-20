use std::error::Error as StdError;
use std::marker::Unpin;
use std::{cmp, io};
use std::{
    pin::Pin,
    task::{self, Poll},
};

use bytes::{Buf, Bytes};

use http::{Request, Response};
use hyper::service::Service;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};

use crate::http::body::{Body, HyperBody};
use crate::rt::TokioIo;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(feature = "http2")]
use crate::rt::TokioExecutor;
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

impl HttpBuilder {
    /// Bind a connection together with a [`Service`].
    pub async fn serve_connection<I, S, B>(&self, mut io: I, service: S) -> Result<()>
    where
        S: Service<Request<HyperBody>, Response = Response<B>> + Send,
        S::Future: Send + 'static,
        S::Error: Into<Box<dyn StdError + Send + Sync>>,
        B: Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<Box<dyn StdError + Send + Sync>>,
        I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        #[derive(Debug)]
        enum Protocol {
            H1,
            H2,
        }

        let mut buf = Vec::new();
        let protocol = loop {
            if buf.len() < 24 {
                io.read_buf(&mut buf).await?;

                let len = buf.len().min(H2_PREFACE.len());

                if buf[0..len] != H2_PREFACE[0..len] {
                    break Protocol::H1;
                }
            } else {
                break Protocol::H2;
            }
        };
        let io = Rewind::new_buffered(io, Bytes::from(buf));
        match protocol {
            Protocol::H1 => {
                #[cfg(not(feature = "http1"))]
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "http1 feature not enabled").into());
                #[cfg(feature = "http1")]
                self.http1
                    .serve_connection(TokioIo::new(io), service)
                    .with_upgrades()
                    .await?;
            }
            Protocol::H2 => {
                #[cfg(not(feature = "http2"))]
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "http2 feature not enabled").into());
                #[cfg(feature = "http2")]
                self.http2.serve_connection(TokioIo::new(io), service).await?;
            }
        }

        Ok(())
    }
}

// from https://github.com/hyperium/hyper-util/pull/11/files#diff-1bd3ef8e9a23396b76bdb4ec6ab5aba4c48dd0511d287e485148a90170c6b4fd
/// Combine a buffer with an IO, rewinding reads to use the buffer.
#[derive(Debug)]
struct Rewind<T> {
    pre: Option<Bytes>,
    inner: T,
}

impl<T> Rewind<T> {
    fn new_buffered(io: T, buf: Bytes) -> Self {
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
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
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
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}
