use std::{error::Error as StdError, marker::Unpin};

use http::{Request, Response};
use hyper::service::Service;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, BufReader};

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
        enum Protocol {
            H1,
            H2,
        }

        let mut buf = Vec::new();
        let mut buf_reader = BufReader::new(&mut io);
        let protocol = if buf_reader.read_exact(&mut buf).await.is_ok() {
            if buf == H2_PREFACE {
                Protocol::H2
            } else {
                Protocol::H1
            }
        } else {
            Protocol::H1
        };
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
